cmake_minimum_required(VERSION 3.20)
project(terlan_cpp_metadata_extractor LANGUAGES C CXX)

find_package(LLVM REQUIRED CONFIG)
find_package(Clang REQUIRED CONFIG)

add_executable(terlan-cpp-metadata-extractor src/main.cpp)
target_compile_features(terlan-cpp-metadata-extractor PRIVATE cxx_std_17)
target_include_directories(
  terlan-cpp-metadata-extractor
  SYSTEM PRIVATE
  ${LLVM_INCLUDE_DIRS}
  ${CLANG_INCLUDE_DIRS}
)
add_definitions(${LLVM_DEFINITIONS})
target_link_libraries(
  terlan-cpp-metadata-extractor
  PRIVATE
  clangAST
  clangASTMatchers
  clangBasic
  clangFrontend
  clangIndex
  clangLex
  clangTooling
  LLVMSupport
)

if(MSVC)
  target_compile_options(terlan-cpp-metadata-extractor PRIVATE /W4 /WX)
else()
  target_compile_options(
    terlan-cpp-metadata-extractor
    PRIVATE
    -Wall
    -Wextra
    -Werror
  )
endif()
