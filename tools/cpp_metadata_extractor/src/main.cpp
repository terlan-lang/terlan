#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <map>
#include <string>
#include <system_error>
#include <utility>
#include <vector>

#include "clang/AST/ASTContext.h"
#include "clang/AST/Attr.h"
#include "clang/AST/DeclCXX.h"
#include "clang/AST/RawCommentList.h"
#include "clang/AST/RecursiveASTVisitor.h"
#include "clang/AST/Stmt.h"
#include "clang/AST/Type.h"
#include "clang/ASTMatchers/ASTMatchFinder.h"
#include "clang/ASTMatchers/ASTMatchers.h"
#include "clang/Basic/ExceptionSpecificationType.h"
#include "clang/Basic/SourceManager.h"
#include "clang/Basic/TargetInfo.h"
#include "clang/Basic/Version.h"
#include "clang/Lex/Lexer.h"
#include "clang/Tooling/CommonOptionsParser.h"
#include "clang/Tooling/CompilationDatabase.h"
#include "clang/Tooling/Tooling.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/JSON.h"
#include "llvm/Support/Path.h"
#include "llvm/ADT/SmallString.h"
#include "llvm/Support/raw_ostream.h"

namespace {

using clang::ASTContext;
using clang::CXXMethodDecl;
using clang::CXXRecordDecl;
using clang::Decl;
using clang::FunctionDecl;
using clang::EnumDecl;
using clang::FunctionProtoType;
using clang::ParmVarDecl;
using clang::QualType;
using clang::SourceLocation;
using clang::SourceManager;
using clang::ast_matchers::MatchFinder;
using clang::ast_matchers::cxxRecordDecl;
using clang::ast_matchers::functionDecl;
using clang::ast_matchers::enumDecl;
using clang::ast_matchers::isDefinition;
using clang::ast_matchers::isImplicit;
using clang::ast_matchers::unless;
using clang::tooling::ClangTool;
using clang::tooling::CommonOptionsParser;
using clang::tooling::CompileCommand;

llvm::cl::OptionCategory ToolCategory("terlan-cpp-metadata-extractor options");
llvm::cl::opt<std::string> OutputPath(
    "output", llvm::cl::desc("Output normalized metadata JSON path"),
    llvm::cl::value_desc("path"), llvm::cl::Required,
    llvm::cl::cat(ToolCategory));
llvm::cl::list<std::string> HeaderPaths(
    "header", llvm::cl::desc("Header whose declarations should be emitted"),
    llvm::cl::value_desc("path"), llvm::cl::OneOrMore,
    llvm::cl::cat(ToolCategory));
llvm::cl::opt<std::string> Namespace(
    "namespace", llvm::cl::desc("C++ namespace selected by package policy"),
    llvm::cl::value_desc("qualified-name"), llvm::cl::Required,
    llvm::cl::cat(ToolCategory));
llvm::cl::opt<bool> PublicOnly(
    "public-only",
    llvm::cl::desc("Emit only declarations with public or namespace access"),
    llvm::cl::init(false), llvm::cl::cat(ToolCategory));
llvm::cl::opt<bool> ExactHeaders(
    "exact-headers",
    llvm::cl::desc("Match selected headers by canonical path instead of basename"),
    llvm::cl::init(false), llvm::cl::cat(ToolCategory));
llvm::cl::opt<std::string> HeaderRoot(
    "header-root",
    llvm::cl::desc("Root removed from emitted selected-header source paths"),
    llvm::cl::value_desc("path"), llvm::cl::init(""),
    llvm::cl::cat(ToolCategory));

struct ExtractedSymbol {
  std::string id;
  std::string overload_set;
  llvm::json::Object value;
};

/// Returns a stable spelling for one C++ member access level.
std::string access_spelling(clang::AccessSpecifier access) {
  switch (access) {
    case clang::AS_public:
      return "public";
    case clang::AS_protected:
      return "protected";
    case clang::AS_private:
      return "private";
    case clang::AS_none:
      return "none";
  }
  return "none";
}

/// Returns whether a declaration is visible through public C++ API access.
bool has_public_access(const Decl& declaration) {
  const clang::AccessSpecifier access = declaration.getAccess();
  return access == clang::AS_public || access == clang::AS_none;
}

/// Returns a deterministic package-relative spelling for a source path.
std::string file_name(llvm::StringRef path) {
  return llvm::sys::path::filename(path).str();
}

/// Returns a canonical absolute path with platform separators normalized.
std::string canonical_path(llvm::StringRef path) {
  llvm::SmallString<256> absolute(path);
  if (std::error_code error = llvm::sys::fs::make_absolute(absolute)) {
    return {};
  }
  llvm::sys::path::remove_dots(absolute, true);
  std::string normalized = absolute.str().str();
  std::replace(normalized.begin(), normalized.end(), '\\', '/');
  return normalized;
}

/// Returns the selected source path or an empty string for an unselected file.
std::string selected_source_path(const Decl& declaration,
                                 const SourceManager& sources) {
  SourceLocation location = sources.getSpellingLoc(declaration.getLocation());
  if (location.isInvalid()) {
    return {};
  }
  const std::string source = canonical_path(sources.getFilename(location));
  if (source.empty()) {
    return {};
  }
  for (const std::string& header : HeaderPaths) {
    const std::string selected = canonical_path(header);
    if (ExactHeaders ? source == selected
                     : file_name(source) == file_name(selected)) {
      return source;
    }
  }
  return {};
}

/// Removes the selected package namespace from source-oriented type spellings.
std::string source_type_spelling(QualType type, clang::PrintingPolicy policy) {
  std::string spelling = type.getAsString(policy);
  const std::string prefix = Namespace + "::";
  std::size_t position = 0;
  while ((position = spelling.find(prefix, position)) != std::string::npos) {
    spelling.erase(position, prefix.size());
  }
  return spelling;
}

/// Returns whether a declaration was spelled in the selected header.
bool is_selected_header(const Decl& declaration, const SourceManager& sources) {
  return !selected_source_path(declaration, sources).empty();
}

/// Renders one selected source path relative to the requested header root.
std::string rendered_source_path(const Decl& declaration,
                                 const SourceManager& sources) {
  const std::string source = selected_source_path(declaration, sources);
  if (HeaderRoot.empty()) {
    return file_name(source);
  }
  std::string root = canonical_path(HeaderRoot);
  if (!root.empty() && root.back() != '/') {
    root.push_back('/');
  }
  if (!root.empty() && source.rfind(root, 0) == 0) {
    return source.substr(root.size());
  }
  return file_name(source);
}

/// Extracts maintained-Clang annotation payloads in deterministic order.
llvm::json::Array annotations(const Decl& declaration) {
  std::vector<std::string> values;
  for (const clang::Attr* attribute : declaration.attrs()) {
    if (const auto* annotate = llvm::dyn_cast<clang::AnnotateAttr>(attribute)) {
      values.push_back(annotate->getAnnotation().str());
    }
  }
  std::sort(values.begin(), values.end());
  llvm::json::Array result;
  for (const std::string& value : values) {
    result.push_back(value);
  }
  return result;
}

/// Extracts the declaration's brief documentation through Clang comments.
std::string documentation(const Decl& declaration, ASTContext& context) {
  const clang::RawComment* comment = context.getRawCommentForDeclNoCache(&declaration);
  if (comment == nullptr) {
    return "Undocumented upstream declaration.";
  }
  std::string brief = comment->getBriefText(context);
  return brief.empty() ? "Undocumented upstream declaration." : brief;
}

/// Converts a source location into the normalized one-based location object.
llvm::json::Object source_location(const Decl& declaration,
                                   const SourceManager& sources) {
  SourceLocation location = sources.getSpellingLoc(declaration.getLocation());
  llvm::json::Object result;
  result["path"] = rendered_source_path(declaration, sources);
  result["line"] = static_cast<std::int64_t>(sources.getSpellingLineNumber(location));
  result["column"] =
      static_cast<std::int64_t>(sources.getSpellingColumnNumber(location));
  return result;
}

/// Converts a Clang qualified type into the normalized structured type shape.
llvm::json::Object type_metadata(QualType type, ASTContext& context) {
  clang::PrintingPolicy policy(context.getLangOpts());
  policy.SuppressTagKeyword = true;
  QualType canonical = type.getCanonicalType();
  QualType unwrapped = type;
  std::string reference = "none";
  if (type->isLValueReferenceType()) {
    reference = "lvalue";
    unwrapped = type->getPointeeType();
  } else if (type->isRValueReferenceType()) {
    reference = "rvalue";
    unwrapped = type->getPointeeType();
  }
  std::size_t pointer_depth = 0;
  QualType pointer = unwrapped;
  while (!pointer.isNull() && pointer->isPointerType()) {
    ++pointer_depth;
    pointer = pointer->getPointeeType();
  }

  llvm::json::Object result;
  result["spelling"] = source_type_spelling(type, policy);
  result["canonical"] = canonical.getAsString(policy);
  result["is_const"] = unwrapped.isConstQualified();
  result["pointer_depth"] = static_cast<std::int64_t>(pointer_depth);
  result["reference"] = reference;
  result["function_pointer"] = type->isFunctionPointerType();
  result["template_dependent"] = type->isDependentType();
  if (canonical->isEnumeralType()) {
    result["enum_type"] = true;
  }
  return result;
}

/// Returns the source spelling for a default parameter expression.
std::string default_expression(const ParmVarDecl& parameter,
                               const MatchFinder::MatchResult& match) {
  if (!parameter.hasDefaultArg()) {
    return {};
  }
  const clang::Expr* expression = parameter.getDefaultArg();
  if (expression == nullptr || expression->getSourceRange().isInvalid()) {
    return {};
  }
  clang::CharSourceRange range =
      clang::CharSourceRange::getTokenRange(expression->getSourceRange());
  return clang::Lexer::getSourceText(range, *match.SourceManager,
                                     match.Context->getLangOpts())
      .str();
}

/// Derives input/output direction from maintained Clang annotation facts.
std::string parameter_direction(const ParmVarDecl& parameter) {
  for (const clang::Attr* attribute : parameter.attrs()) {
    if (const auto* annotate = llvm::dyn_cast<clang::AnnotateAttr>(attribute)) {
      if (annotate->getAnnotation() == "CV_OUT") {
        return "output";
      }
      if (annotate->getAnnotation() == "CV_IN_OUT") {
        return "in_out";
      }
    }
  }
  return "input";
}

/// Extracts one callable parameter list from the Clang declaration.
llvm::json::Array parameters(const FunctionDecl& function,
                             const MatchFinder::MatchResult& match) {
  llvm::json::Array result;
  for (const ParmVarDecl* parameter : function.parameters()) {
    llvm::json::Object value;
    value["name"] = parameter->getNameAsString();
    value["ty"] = type_metadata(parameter->getType(), *match.Context);
    value["direction"] = parameter_direction(*parameter);
    std::string expression = default_expression(*parameter, match);
    if (!expression.empty()) {
      value["default"] = expression;
    }
    result.push_back(std::move(value));
  }
  return result;
}

/// Extracts named template parameters from a function template declaration.
llvm::json::Array template_parameters(const FunctionDecl& function) {
  llvm::json::Array result;
  const clang::FunctionTemplateDecl* templated =
      function.getDescribedFunctionTemplate();
  if (templated == nullptr) {
    return result;
  }
  std::size_t index = 0;
  for (const clang::NamedDecl* parameter : *templated->getTemplateParameters()) {
    std::string name = parameter->getNameAsString();
    result.push_back(name.empty() ? "parameter_" + std::to_string(index) : name);
    ++index;
  }
  return result;
}

/// Produces a stable symbol ID from kind, qualified name, and canonical inputs.
std::string symbol_id(const FunctionDecl& function, ASTContext& context) {
  std::string kind = llvm::isa<CXXMethodDecl>(function) ? "method:" : "function:";
  std::string value = kind + function.getQualifiedNameAsString() + "(";
  bool first = true;
  for (const ParmVarDecl* parameter : function.parameters()) {
    if (!first) {
      value += ",";
    }
    first = false;
    QualType type = parameter->getType();
    clang::PrintingPolicy policy(context.getLangOpts());
    policy.SuppressTagKeyword = true;
    value += type->isDependentType()
                 ? source_type_spelling(type, policy)
                 : type.getCanonicalType().getAsString(policy);
  }
  if (function.isVariadic()) {
    if (!function.parameters().empty()) {
      value += ",";
    }
    value += "...";
  }
  value += ")";
  return value;
}

/// Collects direct callable targets from one selected function definition.
class DirectCallCollector final
    : public clang::RecursiveASTVisitor<DirectCallCollector> {
 public:
  /// Creates a collector using the active translation unit's type context.
  explicit DirectCallCollector(ASTContext& context) : context_(context) {}

  /// Records one statically resolved call target.
  bool VisitCallExpr(clang::CallExpr* expression) {
    const FunctionDecl* callee = expression->getDirectCallee();
    if (callee == nullptr || callee->isImplicit()) {
      return true;
    }
    const FunctionDecl* canonical = callee->getCanonicalDecl();
    const std::string id = symbol_id(*canonical, context_);
    llvm::json::Object value;
    value["id"] = id;
    value["overload_set"] = canonical->getQualifiedNameAsString();
    value["kind"] =
        llvm::isa<CXXMethodDecl>(canonical) ? "method" : "function";
    calls_.emplace(id, std::move(value));
    return true;
  }

  /// Returns direct calls sorted by their stable declaration identity.
  llvm::json::Array take_calls() {
    llvm::json::Array result;
    for (auto& [id, value] : calls_) {
      static_cast<void>(id);
      result.push_back(std::move(value));
    }
    return result;
  }

 private:
  ASTContext& context_;
  std::map<std::string, llvm::json::Object> calls_;
};

/// Extracts statically resolved direct calls from one callable definition.
llvm::json::Array direct_calls(const FunctionDecl& function,
                              ASTContext& context) {
  const FunctionDecl* definition = function.getDefinition();
  if (definition == nullptr || !definition->hasBody()) {
    return {};
  }
  DirectCallCollector collector(context);
  collector.TraverseStmt(definition->getBody());
  return collector.take_calls();
}

/// Reports whether Clang proved a callable to be non-throwing.
bool is_noexcept(const FunctionDecl& function) {
  const auto* prototype = function.getType()->getAs<FunctionProtoType>();
  return prototype != nullptr &&
         clang::isNoexceptExceptionSpec(prototype->getExceptionSpecType());
}

/// Collects declarations from the selected header without parsing C++ itself.
class DeclarationCollector final : public MatchFinder::MatchCallback {
 public:
  /// Records one matched record or callable declaration.
  void run(const MatchFinder::MatchResult& match) override {
    if (const auto* record =
            match.Nodes.getNodeAs<CXXRecordDecl>("record")) {
      collect_record(*record, match);
    }
    if (const auto* enumeration = match.Nodes.getNodeAs<EnumDecl>("enum")) {
      collect_enum(*enumeration, match);
    }
    if (const auto* function =
            match.Nodes.getNodeAs<FunctionDecl>("function")) {
      collect_function(*function, match);
    }
  }

  /// Returns the target triple observed by the Clang AST context.
  const std::string& target_triple() const { return target_triple_; }

  /// Returns all normalized symbols accumulated across translation units.
  std::vector<ExtractedSymbol> take_symbols() { return std::move(symbols_); }

 private:
  /// Records one complete enum and its exact Clang-evaluated discriminants.
  void collect_enum(const EnumDecl& enumeration,
                    const MatchFinder::MatchResult& match) {
    if (!enumeration.isCompleteDefinition() ||
        !is_selected_header(enumeration, *match.SourceManager) ||
        (PublicOnly && !has_public_access(enumeration))) {
      return;
    }
    remember_target(*match.Context);
    llvm::json::Object value;
    std::string qualified = enumeration.getQualifiedNameAsString();
    value["id"] = "enum:" + qualified;
    value["cpp_name"] = enumeration.getNameAsString();
    value["source"] = source_location(enumeration, *match.SourceManager);
    value["kind"] = "enum";
    value["documentation"] = documentation(enumeration, *match.Context);
    value["annotations"] = annotations(enumeration);
    value["overload_set"] = qualified;
    llvm::json::Array enum_values;
    for (const clang::EnumConstantDecl* enumerator : enumeration.enumerators()) {
      llvm::SmallString<32> rendered;
      enumerator->getInitVal().toString(rendered, 10);
      llvm::json::Object normalized;
      normalized["name"] = enumerator->getNameAsString();
      normalized["value"] = rendered.str().str();
      enum_values.push_back(std::move(normalized));
    }
    value["enum_values"] = std::move(enum_values);
    symbols_.push_back(
        ExtractedSymbol{"enum:" + qualified, qualified, std::move(value)});
  }

  /// Records a complete C++ record declared in the selected header.
  void collect_record(const CXXRecordDecl& record,
                      const MatchFinder::MatchResult& match) {
    if (!record.isThisDeclarationADefinition() ||
        !is_selected_header(record, *match.SourceManager) ||
        (PublicOnly && !has_public_access(record))) {
      return;
    }
    remember_target(*match.Context);
    llvm::json::Object value;
    std::string qualified = record.getQualifiedNameAsString();
    value["id"] = "record:" + qualified;
    value["cpp_name"] = record.getNameAsString();
    value["source"] = source_location(record, *match.SourceManager);
    value["kind"] = "record";
    value["documentation"] = documentation(record, *match.Context);
    value["annotations"] = annotations(record);
    value["overload_set"] = qualified;
    llvm::json::Array fields;
    for (const clang::FieldDecl* field : record.fields()) {
      llvm::json::Object normalized;
      normalized["name"] = field->getNameAsString();
      normalized["ty"] = type_metadata(field->getType(), *match.Context);
      normalized["access"] = access_spelling(field->getAccess());
      fields.push_back(std::move(normalized));
    }
    if (!fields.empty()) {
      value["fields"] = std::move(fields);
    }
    llvm::json::Array inheritance;
    for (const clang::CXXBaseSpecifier& base : record.bases()) {
      inheritance.push_back(base.getType().getCanonicalType().getAsString());
    }
    if (!inheritance.empty()) {
      value["inheritance"] = std::move(inheritance);
    }
    symbols_.push_back(
        ExtractedSymbol{"record:" + qualified, qualified, std::move(value)});
  }

  /// Records one C++ function or method declared in the selected header.
  void collect_function(const FunctionDecl& function,
                        const MatchFinder::MatchResult& match) {
    if (!is_selected_header(function, *match.SourceManager) ||
        (PublicOnly && !has_public_access(function)) ||
        function.getFriendObjectKind() != Decl::FOK_None ||
        llvm::isa<clang::CXXConstructorDecl>(function) ||
        llvm::isa<clang::CXXDestructorDecl>(function)) {
      return;
    }
    remember_target(*match.Context);
    std::string qualified = function.getQualifiedNameAsString();
    std::string id = symbol_id(function, *match.Context);
    llvm::json::Object value;
    value["id"] = id;
    value["cpp_name"] = function.getNameAsString();
    value["source"] = source_location(function, *match.SourceManager);
    value["kind"] = llvm::isa<CXXMethodDecl>(function) ? "method" : "function";
    value["documentation"] = documentation(function, *match.Context);
    value["annotations"] = annotations(function);
    value["overload_set"] = qualified;
    if (const auto* method = llvm::dyn_cast<CXXMethodDecl>(&function)) {
      value["receiver"] = method->getParent()->getQualifiedNameAsString();
      value["receiver_mutable"] = !method->isConst();
    }
    value["returns"] = type_metadata(function.getReturnType(), *match.Context);
    value["parameters"] = parameters(function, match);
    value["noexcept"] = is_noexcept(function);
    value["template_parameters"] = template_parameters(function);
    value["variadic"] = function.isVariadic();
    llvm::json::Array calls = direct_calls(function, *match.Context);
    if (!calls.empty()) {
      value["direct_calls"] = std::move(calls);
    }
    symbols_.push_back(ExtractedSymbol{id, qualified, std::move(value)});
  }

  /// Captures the frontend target exactly once for compile provenance.
  void remember_target(const ASTContext& context) {
    if (target_triple_.empty()) {
      target_triple_ = context.getTargetInfo().getTriple().str();
    }
  }

  std::vector<ExtractedSymbol> symbols_;
  std::string target_triple_;
};

/// Returns the first compile command for one source or a stable error.
llvm::Expected<CompileCommand> compile_command(
    CommonOptionsParser& options, llvm::StringRef source) {
  std::vector<CompileCommand> commands =
      options.getCompilations().getCompileCommands(source);
  if (commands.empty()) {
    const llvm::StringRef source_name = llvm::sys::path::filename(source);
    for (const CompileCommand& candidate :
         options.getCompilations().getAllCompileCommands()) {
      if (llvm::sys::path::filename(candidate.Filename) == source_name) {
        commands.push_back(candidate);
      }
    }
  }
  if (commands.empty()) {
    return llvm::createStringError(std::errc::invalid_argument,
                                   "no compile command for %s",
                                   source.str().c_str());
  }
  if (commands.size() != 1) {
    return llvm::createStringError(std::errc::invalid_argument,
                                   "ambiguous compile commands for %s",
                                   source.str().c_str());
  }
  return commands.front();
}

/// Parses structured compile provenance from a tokenized compile command.
llvm::json::Object compile_configuration(const CompileCommand& command,
                                         llvm::StringRef target_triple) {
  std::string standard = "c++14";
  std::string target = target_triple.str();
  std::vector<std::string> roots;
  std::map<std::string, std::string> defines;
  llvm::json::Array arguments;
  for (std::size_t index = 0; index < command.CommandLine.size(); ++index) {
    const std::string& argument = command.CommandLine[index];
    if (argument.rfind("--driver-mode=", 0) == 0) {
      continue;
    }
    arguments.push_back(argument);
    if (argument.rfind("-std=", 0) == 0) {
      standard = llvm::StringRef(argument).drop_front(5).str();
    } else if (argument.rfind("--target=", 0) == 0) {
      target = llvm::StringRef(argument).drop_front(9).str();
    } else if (argument.rfind("-I", 0) == 0 && argument.size() > 2) {
      roots.push_back(argument.substr(2));
    } else if (argument == "-I" && index + 1 < command.CommandLine.size()) {
      roots.push_back(command.CommandLine[index + 1]);
    } else if (argument.rfind("-D", 0) == 0 && argument.size() > 2) {
      llvm::StringRef define(argument);
      define = define.drop_front(2);
      auto pair = define.split('=');
      defines[pair.first.str()] = pair.second.str();
    }
  }
  std::sort(roots.begin(), roots.end());
  roots.erase(std::unique(roots.begin(), roots.end()), roots.end());
  llvm::json::Array include_roots;
  for (const std::string& root : roots) {
    include_roots.push_back(root);
  }
  llvm::json::Object define_values;
  for (const auto& [name, value] : defines) {
    define_values[name] = value.empty() ? llvm::json::Value(nullptr)
                                        : llvm::json::Value(value);
  }
  llvm::json::Object result;
  result["target_triple"] = target;
  result["language_standard"] = standard;
  result["include_roots"] = std::move(include_roots);
  result["defines"] = std::move(define_values);
  result["arguments"] = std::move(arguments);
  return result;
}

/// Sorts, deduplicates, and assigns overload candidate counts.
llvm::json::Array finalize_symbols(std::vector<ExtractedSymbol> symbols) {
  std::sort(symbols.begin(), symbols.end(),
            [](const ExtractedSymbol& left, const ExtractedSymbol& right) {
              return left.id < right.id;
            });
  symbols.erase(
      std::unique(symbols.begin(), symbols.end(),
                  [](const ExtractedSymbol& left,
                     const ExtractedSymbol& right) { return left.id == right.id; }),
      symbols.end());
  std::map<std::string, std::size_t> overload_counts;
  for (const ExtractedSymbol& symbol : symbols) {
    ++overload_counts[symbol.overload_set];
  }
  llvm::json::Array result;
  for (ExtractedSymbol& symbol : symbols) {
    if (symbol.id.rfind("function:", 0) == 0 ||
        symbol.id.rfind("method:", 0) == 0) {
      symbol.value["overload_candidates"] =
          static_cast<std::int64_t>(overload_counts[symbol.overload_set]);
    }
    result.push_back(std::move(symbol.value));
  }
  return result;
}

/// Writes one normalized metadata document atomically enough for build tooling.
llvm::Error write_metadata(llvm::json::Object root) {
  std::error_code error;
  llvm::raw_fd_ostream output(OutputPath, error);
  if (error) {
    return llvm::createStringError(error, "cannot open metadata output");
  }
  output << llvm::formatv("{0:2}\n", llvm::json::Value(std::move(root)));
  return llvm::Error::success();
}

}  // namespace

/// Runs the standalone maintained-Clang metadata extraction pipeline.
int main(int argc, const char** argv) {
  auto parsed = CommonOptionsParser::create(argc, argv, ToolCategory);
  if (!parsed) {
    llvm::errs() << llvm::toString(parsed.takeError()) << '\n';
    return 1;
  }
  CommonOptionsParser& options = parsed.get();
  if (options.getSourcePathList().size() != 1) {
    llvm::errs() << "exactly one translation unit is required\n";
    return 1;
  }
  llvm::Expected<CompileCommand> command =
      compile_command(options, options.getSourcePathList().front());
  if (!command) {
    llvm::errs() << llvm::toString(command.takeError()) << '\n';
    return 1;
  }

  DeclarationCollector collector;
  MatchFinder finder;
  finder.addMatcher(cxxRecordDecl(isDefinition(), unless(isImplicit())).bind("record"),
                    &collector);
  finder.addMatcher(enumDecl(isDefinition(), unless(isImplicit())).bind("enum"),
                    &collector);
  finder.addMatcher(functionDecl(unless(isImplicit())).bind("function"), &collector);
  ClangTool tool(options.getCompilations(), options.getSourcePathList());
  int status = tool.run(clang::tooling::newFrontendActionFactory(&finder).get());
  if (status != 0) {
    return status;
  }

  llvm::json::Object producer;
  producer["name"] = "clang-libtooling";
  producer["version"] = clang::getClangFullVersion();
  producer["format"] = "normalized-ast-json";
  llvm::json::Array sources;
  sources.push_back(file_name(options.getSourcePathList().front()));
  llvm::json::Object root;
  root["schema"] = "terlan.cpp.metadata.v1";
  root["producer"] = std::move(producer);
  root["compile"] = compile_configuration(*command, collector.target_triple());
  root["namespace"] = Namespace.getValue();
  root["header"] = file_name(HeaderPaths.front());
  root["sources"] = std::move(sources);
  root["symbols"] = finalize_symbols(collector.take_symbols());
  if (llvm::Error error = write_metadata(std::move(root))) {
    llvm::errs() << llvm::toString(std::move(error)) << '\n';
    return 1;
  }
  return 0;
}
