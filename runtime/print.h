#ifndef MIMI_PRINT_H
#define MIMI_PRINT_H

#include <stddef.h>
#include <stdint.h>

/*
 * ABI for the language declaration:
 *
 *     extern print : fn(str : []u8)void
 *
 * A slice is passed as a pointer followed by its byte length.
 */
void print(const uint8_t *data, size_t length);

#endif
