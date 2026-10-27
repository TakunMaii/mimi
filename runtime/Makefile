CC ?= cc
AR ?= ar
CFLAGS ?= -std=c11 -Wall -Wextra -O2

all: libprint.a

libprint.a: print.o
	$(AR) rcs $@ $^

print.o: print.c print.h
	$(CC) $(CFLAGS) -c print.c -o $@

clean:
	rm -f print.o libprint.a

.PHONY: all clean
