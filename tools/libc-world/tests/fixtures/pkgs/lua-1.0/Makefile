# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

CC ?= cc
AR ?= ar

all: liblua.a

liblua.a: lua.o
	$(AR) rcs $@ lua.o

lua.o: lua.c
	$(CC) -c lua.c -o lua.o

clean:
	rm -f liblua.a lua.o
