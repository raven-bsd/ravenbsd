# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

CC ?= cc
HOSTCC ?= cc
AR ?= ar

all: libtoy.a

libtoy.a: toy.o
	$(AR) rcs $@ toy.o

toy.o: toy.c
	$(CC) -c toy.c -o toy.o

defconfig:
	printf 'CONFIG_TOY=y\n' > .config

clean:
	rm -f libtoy.a toy.o .config
