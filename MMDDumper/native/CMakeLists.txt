cmake_minimum_required(VERSION 3.20)

project(mmd_oracle_dumper LANGUAGES CXX)

option(MMD_ORACLE_BUILD_DLL "Build the MMD-facing DLL. Requires Windows and MMD_EXPORT_LIB." OFF)
option(MMD_ORACLE_BUILD_MSIMG32_PROXY "Build an MMD-local MSIMG32 proxy loader." OFF)
option(MMD_ORACLE_BUILD_D3D9_PROXY "Build an MMD-local d3d9 proxy render hook." OFF)
option(MMD_ORACLE_BUILD_MMDPLUGIN "Build an MMDPlugin adapter DLL. Requires Windows, MMD_EXPORT_LIB, and MMD_PLUGIN_INCLUDE_DIR." OFF)

add_library(mmd_oracle_core STATIC
  jsonl_writer.cpp
)
target_compile_features(mmd_oracle_core PUBLIC cxx_std_17)
target_include_directories(mmd_oracle_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

if(MSVC)
  target_compile_options(mmd_oracle_core PRIVATE /W4 /WX /utf-8 /EHsc)
else()
  target_compile_options(mmd_oracle_core PRIVATE -Wall -Wextra -Wpedantic -Werror)
endif()

if(MMD_ORACLE_BUILD_D3D9_PROXY)
  if(NOT WIN32)
    message(FATAL_ERROR "MMD_ORACLE_BUILD_D3D9_PROXY requires Windows")
  endif()

  add_library(d3d9 SHARED
    d3d9_proxy.cpp
    d3d9_proxy.def
  )
  target_compile_features(d3d9 PRIVATE cxx_std_17)
  set_target_properties(d3d9 PROPERTIES OUTPUT_NAME "d3d9")

  if(MSVC)
    target_compile_options(d3d9 PRIVATE /W4 /WX /utf-8 /EHsc)
  endif()
endif()

if(MMD_ORACLE_BUILD_MSIMG32_PROXY)
  if(NOT WIN32)
    message(FATAL_ERROR "MMD_ORACLE_BUILD_MSIMG32_PROXY requires Windows")
  endif()

  add_library(msimg32 SHARED
    msimg32_proxy.cpp
    msimg32_proxy.def
  )
  target_compile_features(msimg32 PRIVATE cxx_std_17)
  set_target_properties(msimg32 PROPERTIES OUTPUT_NAME "MSIMG32")

  if(MSVC)
    target_compile_options(msimg32 PRIVATE /W4 /WX /utf-8 /EHsc)
  endif()
endif()

add_executable(mmd_oracle_jsonl_writer_test
  tests/jsonl_writer_test.cpp
)
target_link_libraries(mmd_oracle_jsonl_writer_test PRIVATE mmd_oracle_core)

enable_testing()
add_test(NAME mmd_oracle_jsonl_writer_test COMMAND mmd_oracle_jsonl_writer_test)

if(MMD_ORACLE_BUILD_DLL)
  if(NOT WIN32)
    message(FATAL_ERROR "MMD_ORACLE_BUILD_DLL requires Windows")
  endif()
  if(NOT DEFINED MMD_EXPORT_LIB)
    message(FATAL_ERROR "MMD_EXPORT_LIB must point to local MMDExport.lib")
  endif()

  add_library(mmd_oracle_dumper SHARED
    dllmain.cpp
    mmd_export_provider.cpp
  )
  target_link_libraries(mmd_oracle_dumper PRIVATE mmd_oracle_core "${MMD_EXPORT_LIB}")
  target_compile_features(mmd_oracle_dumper PRIVATE cxx_std_17)
  target_include_directories(mmd_oracle_dumper PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})

  if(MSVC)
    target_compile_options(mmd_oracle_dumper PRIVATE /W4 /WX /utf-8 /EHsc)
  endif()
endif()

if(MMD_ORACLE_BUILD_MMDPLUGIN)
  if(NOT WIN32)
    message(FATAL_ERROR "MMD_ORACLE_BUILD_MMDPLUGIN requires Windows")
  endif()
  if(NOT DEFINED MMD_EXPORT_LIB)
    message(FATAL_ERROR "MMD_EXPORT_LIB must point to local MMDExport.lib")
  endif()
  if(NOT DEFINED MMD_PLUGIN_INCLUDE_DIR)
    message(FATAL_ERROR "MMD_PLUGIN_INCLUDE_DIR must point to the local MMDPlugin developer header directory")
  endif()

  add_library(mmd_oracle_plugin SHARED
    dllmain.cpp
    mmd_export_provider.cpp
    mmd_plugin_dumper.cpp
  )
  target_link_libraries(mmd_oracle_plugin PRIVATE mmd_oracle_core "${MMD_EXPORT_LIB}")
  target_compile_features(mmd_oracle_plugin PRIVATE cxx_std_17)
  target_include_directories(mmd_oracle_plugin PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/compat
    ${CMAKE_CURRENT_SOURCE_DIR}
    "${MMD_PLUGIN_INCLUDE_DIR}"
  )
  target_compile_definitions(mmd_oracle_plugin PRIVATE MAKE_MMD_PLUGIN NOMINMAX _SILENCE_CXX17_RESULT_OF_DEPRECATION_WARNING)
  set_target_properties(mmd_oracle_plugin PROPERTIES OUTPUT_NAME "mmd_oracle_plugin")

  if(MSVC)
    target_compile_options(mmd_oracle_plugin PRIVATE /W4 /WX /utf-8 /EHsc)
  endif()
endif()
