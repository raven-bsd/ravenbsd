# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

cmake_minimum_required(VERSION 3.16)
project(pcre2fixture C)
add_library(pcre2 STATIC pcre2.c)
install(TARGETS pcre2 ARCHIVE DESTINATION lib)
