#include "tree_sitter/parser.h"
#include <stdint.h>
#include <stdlib.h>
#include <wctype.h>
#include "sql_identifier_ranges.h"

enum { SQL_RAW_BLOCK, INTERPOLATED_STRING };

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

// Strings are opaque editor tokens, as before, but expression islands may
// themselves contain strings, comments and balanced braces.
static bool string_island(TSLexer *lexer, unsigned nesting, bool *complex);
static bool interpolation_string(TSLexer *lexer, unsigned nesting, bool *complex) {
  if (nesting >= 128 || lexer->lookahead != '"') return false;
  advance(lexer);
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    advance(lexer);
    if (c == '"') return true;
    if (c == '\\') {
      if (lexer->eof(lexer)) return false;
      advance(lexer);
    } else if (c == '$' && lexer->lookahead == '{') {
      advance(lexer);
      if (!string_island(lexer, nesting + 1, complex)) return false;
    }
  }
  return false;
}
static bool string_island(TSLexer *lexer, unsigned nesting, bool *complex) {
  unsigned braces = 1;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    if (c == '"') {
      *complex = true;
      if (!interpolation_string(lexer, nesting, complex)) return false;
      continue;
    }
    advance(lexer);
    if (c == '%' || (c == '/' && lexer->lookahead == '/')) {
      while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
        if (lexer->lookahead == '"') *complex = true;
        advance(lexer);
      }
    } else if (c == '/' && lexer->lookahead == '*') {
      advance(lexer);
      bool closed = false;
      while (!lexer->eof(lexer)) {
        int32_t previous = lexer->lookahead;
        if (previous == '"') *complex = true;
        advance(lexer);
        if (previous == '*' && lexer->lookahead == '/') {
          advance(lexer); closed = true; break;
        }
      }
      if (!closed) return false;
    } else if (c == '{') braces++;
    else if (c == '}' && --braces == 0) return true;
  }
  return false;
}
static bool ascii_letter(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}
static bool tag_character(int32_t c) {
  return ascii_letter(c) || (c >= '0' && c <= '9');
}
static bool identifier_character(int32_t c) {
  if (c < 0x80) return tag_character(c) || c == '$';
  size_t low = 0, high = sizeof(sql_identifier_ranges) / sizeof(sql_identifier_ranges[0]);
  while (low < high) {
    size_t middle = low + (high - low) / 2;
    if (c < sql_identifier_ranges[middle][0]) high = middle;
    else if (c > sql_identifier_ranges[middle][1]) low = middle + 1;
    else return true;
  }
  return false;
}

// The opening quote has not yet been consumed. SQL quotes double their
// delimiter; interpolation strings use Terlan backslash escapes instead.
static bool quoted(TSLexer *lexer, bool sql) {
  int32_t quote = lexer->lookahead;
  advance(lexer);
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    advance(lexer);
    if (!sql && quote == '"' && c == '\\') {
      if (lexer->eof(lexer)) return false;
      advance(lexer);
    } else if (c == quote) {
      if (sql && lexer->lookahead == quote) advance(lexer);
      else return true;
    }
  }
  return false;
}

// Called immediately after the opening ${. Expression validity belongs to
// compiler lowering; here only its closing delimiter is needed.
static bool interpolation(TSLexer *lexer) {
  size_t depth = 1;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    if (c == '\'' || c == '"') {
      if (!quoted(lexer, false)) return false;
      continue;
    }
    advance(lexer);
    if (c == '{') depth++;
    if (c == '}' && --depth == 0) return true;
  }
  return false;
}

// Called after /*. SQL block comments nest and hide all other syntax.
static bool block_comment(TSLexer *lexer) {
  size_t depth = 1;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    advance(lexer);
    if (c == '/' && lexer->lookahead == '*') {
      advance(lexer);
      depth++;
    } else if (c == '*' && lexer->lookahead == '/') {
      advance(lexer);
      if (--depth == 0) return true;
    }
  }
  return false;
}

// Called after a boundary-eligible $. Tags have no fixed length limit. A
// partial tag that lacks the final $ is ordinary SQL text, not an error.
static bool dollar_region(TSLexer *lexer) {
  size_t length = 1, capacity = 32;
  int32_t *tag = malloc(capacity * sizeof(*tag));
  if (!tag) return false;
  tag[0] = '$';
  while (tag_character(lexer->lookahead)) {
    if (length + 1 == capacity) {
      if (capacity > SIZE_MAX / (2 * sizeof(*tag))) { free(tag); return false; }
      capacity *= 2;
      int32_t *grown = realloc(tag, capacity * sizeof(*tag));
      if (!grown) { free(tag); return false; }
      tag = grown;
    }
    tag[length++] = lexer->lookahead;
    advance(lexer);
  }
  if (lexer->lookahead != '$') { free(tag); return true; }
  tag[length++] = '$';
  advance(lexer);
  size_t matched = 0;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    advance(lexer);
    if (c == tag[matched]) matched++;
    else matched = c == '$' ? 1 : 0;
    if (matched == length) { free(tag); return true; }
  }
  free(tag);
  return false;
}

void *tree_sitter_terlan_external_scanner_create(void) { return NULL; }
void tree_sitter_terlan_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_terlan_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload; (void)buffer; return 0;
}
void tree_sitter_terlan_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  (void)payload; (void)buffer; (void)length;
}

bool tree_sitter_terlan_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  (void)payload;
  while (iswspace(lexer->lookahead)) lexer->advance(lexer, true);
  if (valid_symbols[INTERPOLATED_STRING] && lexer->lookahead == '"') {
    // Preserve ordinary string-pattern precedence unless nested quotes need us.
    bool complex = false;
    if (!interpolation_string(lexer, 0, &complex) || !complex) return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = INTERPOLATED_STRING;
    return true;
  }
  if (!valid_symbols[SQL_RAW_BLOCK]) return false;
  if (lexer->lookahead != '{') return false;
  advance(lexer);
  size_t depth = 1;
  bool previous_identifier = false;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    if (c == '\'' || c == '"') {
      if (!quoted(lexer, true)) return false;
      previous_identifier = false;
      continue;
    }
    advance(lexer);
    if (c == '/' && lexer->lookahead == '*') {
      advance(lexer);
      if (!block_comment(lexer)) return false;
      previous_identifier = false;
      continue;
    }
    if (c == '-' && lexer->lookahead == '-') {
      while (!lexer->eof(lexer) && lexer->lookahead != '\n') advance(lexer);
      previous_identifier = false;
      continue;
    }
    if (c == '$' && lexer->lookahead == '{') {
      advance(lexer);
      if (!interpolation(lexer)) return false;
      previous_identifier = false;
      continue;
    }
    if (c == '$' && !previous_identifier && (lexer->lookahead == '$' || ascii_letter(lexer->lookahead))) {
      if (!dollar_region(lexer)) return false;
      // Both a closed delimiter and a partial tag end in identifier characters.
      previous_identifier = true;
      continue;
    }
    if (c == '\\') {
      if (lexer->eof(lexer)) return false;
      previous_identifier = identifier_character(lexer->lookahead);
      advance(lexer);
      continue;
    }
    if (c == '{') depth++;
    if (c == '}' && --depth == 0) {
      lexer->mark_end(lexer);
      lexer->result_symbol = SQL_RAW_BLOCK;
      return true;
    }
    previous_identifier = identifier_character(c);
  }
  return false;
}
