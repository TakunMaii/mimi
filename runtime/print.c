#include "print.h"

#include <stdio.h>

void print(const uint8_t *data, size_t length) {
    if (data == NULL || length == 0) {
        return;
    }

    (void)fwrite(data, sizeof(*data), length, stdout);
    (void)fflush(stdout);
}
