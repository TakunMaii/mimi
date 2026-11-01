#define _XOPEN_SOURCE 700
#include <stdint.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

typedef struct {
    uint8_t *data;
    uint64_t len;
    uint64_t capacity;
    uint8_t released;
} MimiByteBuffer;

typedef struct {
    uint8_t *ptr;
    uint64_t len;
} MimiRawSlice;

typedef struct MimiArenaBlock {
    struct MimiArenaBlock *next;
    uint64_t capacity;
    uint64_t used;
    unsigned char data[];
} MimiArenaBlock;

typedef struct {
    MimiArenaBlock *head;
    void *last;
} MimiArena;

typedef struct {
    uint64_t hash;
    uint8_t *key;
    uint64_t key_length;
    uint64_t value;
    uint8_t occupied;
} MimiMapEntry;

typedef struct {
    MimiMapEntry *entries;
    size_t capacity;
    size_t length;
} MimiStringMap;

typedef struct {
    uint64_t *items;
    size_t capacity;
    size_t length;
} MimiU64Vector;

typedef struct {
    char **items;
    size_t length;
    size_t capacity;
} MimiProcessArgs;

void *mimi_buffer_empty(void) {
    return NULL;
}

void mimi_slice_from_raw(uint8_t *ptr, uint64_t length, MimiRawSlice *output) {
    output->ptr = ptr;
    output->len = length;
}

int mimi_buffer_resize(MimiByteBuffer *buffer, uint64_t size) {
    void *resized = realloc(buffer->data, (size_t)size);
    if (resized == NULL && size != 0) {
        return 0;
    }
    buffer->data = resized;
    return 1;
}

void mimi_buffer_release(MimiByteBuffer *buffer) {
    free(buffer->data);
    buffer->data = NULL;
}

static void mimi_write_bytes(FILE *stream, const uint8_t *data, uint64_t length) {
    if (length != 0) {
        fwrite(data, 1, (size_t)length, stream);
    }
}

void mimi_stderr_write(const uint8_t *data, uint64_t length) {
    mimi_write_bytes(stderr, data, length);
}

void mimi_diagnostic_print(
    const uint8_t *code,
    uint64_t code_length,
    const uint8_t *message,
    uint64_t message_length,
    const uint8_t *path,
    uint64_t path_length,
    const uint8_t *source,
    uint64_t source_length,
    uint64_t start,
    uint64_t end
) {
    FILE *stream = stderr;
    uint64_t offset = start < source_length ? start : source_length;
    uint64_t line_start = offset;
    uint64_t line_end = offset;
    uint64_t line = 1;
    while (line_start > 0 && source[line_start - 1] != '\n') {
        line_start--;
    }
    for (uint64_t index = 0; index < line_start; index++) {
        if (source[index] == '\n') {
            line++;
        }
    }
    line_end = offset;
    while (line_end < source_length && source[line_end] != '\n') {
        line_end++;
    }
    uint64_t column = offset - line_start + 1;
    uint64_t highlight_end = end < line_end ? end : line_end;
    uint64_t highlight_length = highlight_end > offset ? highlight_end - offset : 1;

    fputs("error[", stream);
    mimi_write_bytes(stream, code, code_length);
    fputs("]: ", stream);
    mimi_write_bytes(stream, message, message_length);
    fputs("\n --> ", stream);
    mimi_write_bytes(stream, path, path_length);
    fprintf(stream, ":%llu:%llu\n  |\n%llu | ",
        (unsigned long long)line,
        (unsigned long long)column,
        (unsigned long long)line);
    const uint8_t *line_data = source;
    if (line_start != 0) {
        line_data = source + line_start;
    }
    mimi_write_bytes(stream, line_data, line_end - line_start);
    fputs("\n  | ", stream);
    for (uint64_t index = 1; index < column; index++) {
        fputc(' ', stream);
    }
    for (uint64_t index = 0; index < highlight_length; index++) {
        fputc(index == 0 ? '^' : '~', stream);
    }
    fputc('\n', stream);
    fflush(stream);
}

static char *mimi_path_copy(const uint8_t *path, uint64_t length) {
    if (length >= SIZE_MAX || (length != 0 && (path == NULL || memchr(path, 0, (size_t)length) != NULL))) {
        return NULL;
    }
    char *copy = malloc((size_t)length + 1);
    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, path, (size_t)length);
    copy[length] = '\0';
    return copy;
}

int mimi_read_file(const uint8_t *path, uint64_t path_length, MimiByteBuffer *buffer) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    FILE *file = fopen(path_copy, "rb");
    free(path_copy);
    if (file == NULL || buffer->released) {
        if (file != NULL) {
            fclose(file);
        }
        return 0;
    }

    uint8_t chunk[4096];
    size_t count;
    int success = 1;
    while ((count = fread(chunk, 1, sizeof(chunk), file)) != 0) {
        uint64_t needed = buffer->len + count;
        if (needed > buffer->capacity) {
            uint64_t capacity = buffer->capacity == 0 ? 16 : buffer->capacity;
            while (capacity < needed) {
                if (capacity > UINT64_MAX / 2) {
                    capacity = needed;
                    break;
                }
                capacity *= 2;
            }
            if (!mimi_buffer_resize(buffer, capacity)) {
                success = 0;
                break;
            }
            buffer->capacity = capacity;
        }
        memcpy(buffer->data + buffer->len, chunk, count);
        buffer->len = needed;
    }
    if (ferror(file)) {
        success = 0;
    }
    fclose(file);
    return success;
}

int mimi_write_file(const uint8_t *path, uint64_t path_length, const uint8_t *data, uint64_t length) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    FILE *file = fopen(path_copy, "wb");
    free(path_copy);
    if (file == NULL) {
        return 0;
    }
    size_t written = fwrite(data, 1, (size_t)length, file);
    int success = written == length && fclose(file) == 0;
    if (written != length) {
        fclose(file);
    }
    return success;
}

int mimi_remove_file(const uint8_t *path, uint64_t path_length) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    int success = remove(path_copy) == 0;
    free(path_copy);
    return success;
}

int mimi_path_exists(const uint8_t *path, uint64_t path_length) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    struct stat info;
    int exists = stat(path_copy, &info) == 0;
    free(path_copy);
    return exists;
}

int mimi_path_is_directory(const uint8_t *path, uint64_t path_length) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    struct stat info;
    int is_directory = stat(path_copy, &info) == 0 && S_ISDIR(info.st_mode);
    free(path_copy);
    return is_directory;
}

int mimi_path_create_directory(const uint8_t *path, uint64_t path_length) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    int created = mkdir(path_copy, 0777) == 0;
    free(path_copy);
    return created;
}

int mimi_path_file_size(const uint8_t *path, uint64_t path_length, uint64_t *size) {
    char *path_copy = mimi_path_copy(path, path_length);
    if (path_copy == NULL) {
        return 0;
    }
    struct stat info;
    int valid = stat(path_copy, &info) == 0 && S_ISREG(info.st_mode) && info.st_size >= 0;
    free(path_copy);
    if (!valid) {
        return 0;
    }
    *size = (uint64_t)info.st_size;
    return 1;
}

int mimi_path_join(const uint8_t *base, uint64_t base_length, const uint8_t *part, uint64_t part_length, MimiByteBuffer *output) {
    if (output == NULL || output->released || part_length > SIZE_MAX || base_length > SIZE_MAX) {
        return 0;
    }
    uint64_t separator = base_length != 0 && base[base_length - 1] != '/' ? 1 : 0;
    if (base_length > UINT64_MAX - separator || base_length + separator > UINT64_MAX - part_length) {
        return 0;
    }
    uint64_t needed = base_length + separator + part_length;
    if (needed > output->capacity) {
        if (!mimi_buffer_resize(output, needed)) {
            return 0;
        }
        output->capacity = needed;
    }
    if (base_length != 0) memcpy(output->data, base, (size_t)base_length);
    if (separator) output->data[base_length] = '/';
    if (part_length != 0) memcpy(output->data + base_length + separator, part, (size_t)part_length);
    output->len = needed;
    return 1;
}

int mimi_path_canonicalize(const uint8_t *path, uint64_t path_length, MimiByteBuffer *output) {
    if (output == NULL || output->released) {
        return 0;
    }
    char *input = mimi_path_copy(path, path_length);
    if (input == NULL) {
        return 0;
    }
    char canonical[PATH_MAX];
    if (realpath(input, canonical) == NULL) {
        free(input);
        return 0;
    }
    free(input);
    size_t canonical_length = strlen(canonical);
    if (canonical_length > output->capacity) {
        if (!mimi_buffer_resize(output, canonical_length)) {
            return 0;
        }
        output->capacity = canonical_length;
    }
    memcpy(output->data, canonical, canonical_length);
    output->len = canonical_length;
    return 1;
}

void *mimi_arena_empty(void) {
    return NULL;
}

int mimi_arena_allocate(MimiArena *arena, uint64_t size, uint64_t alignment) {
    arena->last = NULL;
    if (alignment == 0 || (alignment & (alignment - 1)) != 0 || alignment > 4096) {
        return 0;
    }
    if (size == 0) {
        size = 1;
    }
    MimiArenaBlock *block = arena->head;
    uintptr_t address = 0;
    if (block != NULL) {
        uintptr_t unaligned = (uintptr_t)block->data + block->used;
        address = (unaligned + alignment - 1) & ~(uintptr_t)(alignment - 1);
        uint64_t padding = (uint64_t)(address - unaligned);
        if (padding > block->capacity - block->used ||
            size > block->capacity - block->used - padding) {
            block = NULL;
        } else {
            block->used += padding + size;
        }
    }
    if (block == NULL) {
        if (size > UINT64_MAX - alignment) {
            return 0;
        }
        uint64_t capacity = size + alignment;
        if (capacity < 65536) {
            capacity = 65536;
        }
        if (capacity > SIZE_MAX - sizeof(MimiArenaBlock)) {
            return 0;
        }
        block = malloc(sizeof(MimiArenaBlock) + (size_t)capacity);
        if (block == NULL) {
            return 0;
        }
        block->next = arena->head;
        block->capacity = capacity;
        block->used = 0;
        arena->head = block;
        uintptr_t unaligned = (uintptr_t)block->data;
        address = (unaligned + alignment - 1) & ~(uintptr_t)(alignment - 1);
        block->used = (uint64_t)(address - unaligned) + size;
    }
    arena->last = (void *)address;
    return 1;
}

void mimi_arena_release(MimiArena *arena) {
    MimiArenaBlock *block = arena->head;
    while (block != NULL) {
        MimiArenaBlock *next = block->next;
        free(block);
        block = next;
    }
    arena->head = NULL;
    arena->last = NULL;
}

int mimi_arena_is_empty(MimiArena *arena) {
    return arena->head == NULL && arena->last == NULL;
}

static uint64_t mimi_key_hash(const uint8_t *key, uint64_t length) {
    uint64_t hash = UINT64_C(14695981039346656037);
    for (uint64_t index = 0; index < length; index++) {
        hash ^= key[index];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static int mimi_map_rehash(MimiStringMap *map, size_t capacity) {
    MimiMapEntry *entries = calloc(capacity, sizeof(MimiMapEntry));
    if (entries == NULL) {
        return 0;
    }
    for (size_t index = 0; index < map->capacity; index++) {
        MimiMapEntry entry = map->entries[index];
        if (!entry.occupied) {
            continue;
        }
        size_t slot = (size_t)entry.hash & (capacity - 1);
        while (entries[slot].occupied) {
            slot = (slot + 1) & (capacity - 1);
        }
        entries[slot] = entry;
    }
    free(map->entries);
    map->entries = entries;
    map->capacity = capacity;
    return 1;
}

void *mimi_map_new(void) {
    MimiStringMap *map = calloc(1, sizeof(MimiStringMap));
    if (map == NULL || !mimi_map_rehash(map, 16)) {
        free(map);
        return NULL;
    }
    return map;
}

int mimi_map_set(MimiStringMap *map, const uint8_t *key, uint64_t key_length, uint64_t value) {
    if (map == NULL || key_length > SIZE_MAX) {
        return 0;
    }
    if ((map->length + 1) * 10 >= map->capacity * 7) {
        if (map->capacity > SIZE_MAX / 2 || !mimi_map_rehash(map, map->capacity * 2)) {
            return 0;
        }
    }
    uint64_t hash = mimi_key_hash(key, key_length);
    size_t slot = (size_t)hash & (map->capacity - 1);
    while (map->entries[slot].occupied) {
        MimiMapEntry *entry = &map->entries[slot];
        if (entry->hash == hash && entry->key_length == key_length &&
            (key_length == 0 || memcmp(entry->key, key, (size_t)key_length) == 0)) {
            entry->value = value;
            return 1;
        }
        slot = (slot + 1) & (map->capacity - 1);
    }
    uint8_t *copy = malloc(key_length == 0 ? 1 : (size_t)key_length);
    if (copy == NULL) {
        return 0;
    }
    if (key_length != 0) {
        memcpy(copy, key, (size_t)key_length);
    }
    map->entries[slot] = (MimiMapEntry){hash, copy, key_length, value, 1};
    map->length++;
    return 1;
}

int mimi_map_get(MimiStringMap *map, const uint8_t *key, uint64_t key_length, uint64_t *value) {
    if (map == NULL) {
        return 0;
    }
    uint64_t hash = mimi_key_hash(key, key_length);
    size_t slot = (size_t)hash & (map->capacity - 1);
    while (map->entries[slot].occupied) {
        MimiMapEntry *entry = &map->entries[slot];
        if (entry->hash == hash && entry->key_length == key_length &&
            (key_length == 0 || memcmp(entry->key, key, (size_t)key_length) == 0)) {
            *value = entry->value;
            return 1;
        }
        slot = (slot + 1) & (map->capacity - 1);
    }
    return 0;
}

void mimi_map_release(MimiStringMap *map) {
    if (map == NULL) {
        return;
    }
    for (size_t index = 0; index < map->capacity; index++) {
        if (map->entries[index].occupied) {
            free(map->entries[index].key);
        }
    }
    free(map->entries);
    free(map);
}

void *mimi_vector_new(void) {
    return calloc(1, sizeof(MimiU64Vector));
}

int mimi_vector_push(MimiU64Vector *vector, uint64_t value) {
    if (vector == NULL) {
        return 0;
    }
    if (vector->length == vector->capacity) {
        size_t capacity = vector->capacity == 0 ? 16 : vector->capacity * 2;
        if (capacity < vector->capacity || capacity > SIZE_MAX / sizeof(uint64_t)) {
            return 0;
        }
        uint64_t *items = realloc(vector->items, capacity * sizeof(uint64_t));
        if (items == NULL) {
            return 0;
        }
        vector->items = items;
        vector->capacity = capacity;
    }
    vector->items[vector->length++] = value;
    return 1;
}

int mimi_vector_get(MimiU64Vector *vector, uint64_t index, uint64_t *value) {
    if (vector == NULL || index >= vector->length) {
        return 0;
    }
    *value = vector->items[index];
    return 1;
}

void mimi_vector_release(MimiU64Vector *vector) {
    if (vector == NULL) {
        return;
    }
    free(vector->items);
    free(vector);
}

void *mimi_process_args_new(void) {
    return calloc(1, sizeof(MimiProcessArgs));
}

int mimi_process_args_push(MimiProcessArgs *arguments, const uint8_t *value, uint64_t length) {
    if (arguments == NULL || length >= SIZE_MAX || memchr(value, 0, (size_t)length) != NULL) {
        return 0;
    }
    char *copy = malloc((size_t)length + 1);
    if (copy == NULL) {
        return 0;
    }
    memcpy(copy, value, (size_t)length);
    copy[length] = '\0';
    if (arguments->length == arguments->capacity) {
        size_t capacity = arguments->capacity == 0 ? 8 : arguments->capacity * 2;
        if (capacity < arguments->capacity || capacity > SIZE_MAX / sizeof(char *)) {
            free(copy);
            return 0;
        }
        char **items = realloc(arguments->items, capacity * sizeof(char *));
        if (items == NULL) {
            free(copy);
            return 0;
        }
        arguments->items = items;
        arguments->capacity = capacity;
    }
    arguments->items[arguments->length++] = copy;
    return 1;
}

int mimi_process_run(MimiProcessArgs *arguments, const uint8_t *program, uint64_t length) {
    if (arguments == NULL || length >= SIZE_MAX || memchr(program, 0, (size_t)length) != NULL) {
        return -1;
    }
    char *program_copy = malloc((size_t)length + 1);
    char **argv = calloc(arguments->length + 2, sizeof(char *));
    if (program_copy == NULL || argv == NULL) {
        free(program_copy);
        free(argv);
        return -1;
    }
    memcpy(program_copy, program, (size_t)length);
    program_copy[length] = '\0';
    argv[0] = program_copy;
    for (size_t index = 0; index < arguments->length; index++) {
        argv[index + 1] = arguments->items[index];
    }

    pid_t child = fork();
    if (child == 0) {
        execvp(program_copy, argv);
        _exit(127);
    }
    free(argv);
    free(program_copy);
    if (child < 0) {
        return -1;
    }
    int status;
    while (waitpid(child, &status, 0) < 0) {
        if (errno != EINTR) {
            return -1;
        }
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + WTERMSIG(status);
    }
    return -1;
}

void mimi_process_args_release(MimiProcessArgs *arguments) {
    if (arguments == NULL) {
        return;
    }
    for (size_t index = 0; index < arguments->length; index++) {
        free(arguments->items[index]);
    }
    free(arguments->items);
    free(arguments);
}
typedef struct { uint32_t first; uint32_t last; } MimiUnicodeRange;
static const MimiUnicodeRange MIMI_UNICODE_ALPHABETIC[] = {
    { UINT32_C(65), UINT32_C(90) },
    { UINT32_C(97), UINT32_C(122) },
    { UINT32_C(170), UINT32_C(170) },
    { UINT32_C(181), UINT32_C(181) },
    { UINT32_C(186), UINT32_C(186) },
    { UINT32_C(192), UINT32_C(214) },
    { UINT32_C(216), UINT32_C(246) },
    { UINT32_C(248), UINT32_C(705) },
    { UINT32_C(710), UINT32_C(721) },
    { UINT32_C(736), UINT32_C(740) },
    { UINT32_C(748), UINT32_C(748) },
    { UINT32_C(750), UINT32_C(750) },
    { UINT32_C(837), UINT32_C(837) },
    { UINT32_C(867), UINT32_C(884) },
    { UINT32_C(886), UINT32_C(887) },
    { UINT32_C(890), UINT32_C(893) },
    { UINT32_C(895), UINT32_C(895) },
    { UINT32_C(902), UINT32_C(902) },
    { UINT32_C(904), UINT32_C(906) },
    { UINT32_C(908), UINT32_C(908) },
    { UINT32_C(910), UINT32_C(929) },
    { UINT32_C(931), UINT32_C(1013) },
    { UINT32_C(1015), UINT32_C(1153) },
    { UINT32_C(1162), UINT32_C(1327) },
    { UINT32_C(1329), UINT32_C(1366) },
    { UINT32_C(1369), UINT32_C(1369) },
    { UINT32_C(1376), UINT32_C(1416) },
    { UINT32_C(1456), UINT32_C(1469) },
    { UINT32_C(1471), UINT32_C(1471) },
    { UINT32_C(1473), UINT32_C(1474) },
    { UINT32_C(1476), UINT32_C(1477) },
    { UINT32_C(1479), UINT32_C(1479) },
    { UINT32_C(1488), UINT32_C(1514) },
    { UINT32_C(1519), UINT32_C(1522) },
    { UINT32_C(1552), UINT32_C(1562) },
    { UINT32_C(1568), UINT32_C(1623) },
    { UINT32_C(1625), UINT32_C(1631) },
    { UINT32_C(1646), UINT32_C(1747) },
    { UINT32_C(1749), UINT32_C(1756) },
    { UINT32_C(1761), UINT32_C(1768) },
    { UINT32_C(1773), UINT32_C(1775) },
    { UINT32_C(1786), UINT32_C(1788) },
    { UINT32_C(1791), UINT32_C(1791) },
    { UINT32_C(1808), UINT32_C(1855) },
    { UINT32_C(1869), UINT32_C(1969) },
    { UINT32_C(1994), UINT32_C(2026) },
    { UINT32_C(2036), UINT32_C(2037) },
    { UINT32_C(2042), UINT32_C(2042) },
    { UINT32_C(2048), UINT32_C(2071) },
    { UINT32_C(2074), UINT32_C(2092) },
    { UINT32_C(2112), UINT32_C(2136) },
    { UINT32_C(2144), UINT32_C(2154) },
    { UINT32_C(2160), UINT32_C(2183) },
    { UINT32_C(2185), UINT32_C(2191) },
    { UINT32_C(2199), UINT32_C(2199) },
    { UINT32_C(2208), UINT32_C(2249) },
    { UINT32_C(2260), UINT32_C(2271) },
    { UINT32_C(2275), UINT32_C(2281) },
    { UINT32_C(2288), UINT32_C(2363) },
    { UINT32_C(2365), UINT32_C(2380) },
    { UINT32_C(2382), UINT32_C(2384) },
    { UINT32_C(2389), UINT32_C(2403) },
    { UINT32_C(2417), UINT32_C(2435) },
    { UINT32_C(2437), UINT32_C(2444) },
    { UINT32_C(2447), UINT32_C(2448) },
    { UINT32_C(2451), UINT32_C(2472) },
    { UINT32_C(2474), UINT32_C(2480) },
    { UINT32_C(2482), UINT32_C(2482) },
    { UINT32_C(2486), UINT32_C(2489) },
    { UINT32_C(2493), UINT32_C(2500) },
    { UINT32_C(2503), UINT32_C(2504) },
    { UINT32_C(2507), UINT32_C(2508) },
    { UINT32_C(2510), UINT32_C(2510) },
    { UINT32_C(2519), UINT32_C(2519) },
    { UINT32_C(2524), UINT32_C(2525) },
    { UINT32_C(2527), UINT32_C(2531) },
    { UINT32_C(2544), UINT32_C(2545) },
    { UINT32_C(2556), UINT32_C(2556) },
    { UINT32_C(2561), UINT32_C(2563) },
    { UINT32_C(2565), UINT32_C(2570) },
    { UINT32_C(2575), UINT32_C(2576) },
    { UINT32_C(2579), UINT32_C(2600) },
    { UINT32_C(2602), UINT32_C(2608) },
    { UINT32_C(2610), UINT32_C(2611) },
    { UINT32_C(2613), UINT32_C(2614) },
    { UINT32_C(2616), UINT32_C(2617) },
    { UINT32_C(2622), UINT32_C(2626) },
    { UINT32_C(2631), UINT32_C(2632) },
    { UINT32_C(2635), UINT32_C(2636) },
    { UINT32_C(2641), UINT32_C(2641) },
    { UINT32_C(2649), UINT32_C(2652) },
    { UINT32_C(2654), UINT32_C(2654) },
    { UINT32_C(2672), UINT32_C(2677) },
    { UINT32_C(2689), UINT32_C(2691) },
    { UINT32_C(2693), UINT32_C(2701) },
    { UINT32_C(2703), UINT32_C(2705) },
    { UINT32_C(2707), UINT32_C(2728) },
    { UINT32_C(2730), UINT32_C(2736) },
    { UINT32_C(2738), UINT32_C(2739) },
    { UINT32_C(2741), UINT32_C(2745) },
    { UINT32_C(2749), UINT32_C(2757) },
    { UINT32_C(2759), UINT32_C(2761) },
    { UINT32_C(2763), UINT32_C(2764) },
    { UINT32_C(2768), UINT32_C(2768) },
    { UINT32_C(2784), UINT32_C(2787) },
    { UINT32_C(2809), UINT32_C(2812) },
    { UINT32_C(2817), UINT32_C(2819) },
    { UINT32_C(2821), UINT32_C(2828) },
    { UINT32_C(2831), UINT32_C(2832) },
    { UINT32_C(2835), UINT32_C(2856) },
    { UINT32_C(2858), UINT32_C(2864) },
    { UINT32_C(2866), UINT32_C(2867) },
    { UINT32_C(2869), UINT32_C(2873) },
    { UINT32_C(2877), UINT32_C(2884) },
    { UINT32_C(2887), UINT32_C(2888) },
    { UINT32_C(2891), UINT32_C(2892) },
    { UINT32_C(2902), UINT32_C(2903) },
    { UINT32_C(2908), UINT32_C(2909) },
    { UINT32_C(2911), UINT32_C(2915) },
    { UINT32_C(2929), UINT32_C(2929) },
    { UINT32_C(2946), UINT32_C(2947) },
    { UINT32_C(2949), UINT32_C(2954) },
    { UINT32_C(2958), UINT32_C(2960) },
    { UINT32_C(2962), UINT32_C(2965) },
    { UINT32_C(2969), UINT32_C(2970) },
    { UINT32_C(2972), UINT32_C(2972) },
    { UINT32_C(2974), UINT32_C(2975) },
    { UINT32_C(2979), UINT32_C(2980) },
    { UINT32_C(2984), UINT32_C(2986) },
    { UINT32_C(2990), UINT32_C(3001) },
    { UINT32_C(3006), UINT32_C(3010) },
    { UINT32_C(3014), UINT32_C(3016) },
    { UINT32_C(3018), UINT32_C(3020) },
    { UINT32_C(3024), UINT32_C(3024) },
    { UINT32_C(3031), UINT32_C(3031) },
    { UINT32_C(3072), UINT32_C(3084) },
    { UINT32_C(3086), UINT32_C(3088) },
    { UINT32_C(3090), UINT32_C(3112) },
    { UINT32_C(3114), UINT32_C(3129) },
    { UINT32_C(3133), UINT32_C(3140) },
    { UINT32_C(3142), UINT32_C(3144) },
    { UINT32_C(3146), UINT32_C(3148) },
    { UINT32_C(3157), UINT32_C(3158) },
    { UINT32_C(3160), UINT32_C(3162) },
    { UINT32_C(3164), UINT32_C(3165) },
    { UINT32_C(3168), UINT32_C(3171) },
    { UINT32_C(3200), UINT32_C(3203) },
    { UINT32_C(3205), UINT32_C(3212) },
    { UINT32_C(3214), UINT32_C(3216) },
    { UINT32_C(3218), UINT32_C(3240) },
    { UINT32_C(3242), UINT32_C(3251) },
    { UINT32_C(3253), UINT32_C(3257) },
    { UINT32_C(3261), UINT32_C(3268) },
    { UINT32_C(3270), UINT32_C(3272) },
    { UINT32_C(3274), UINT32_C(3276) },
    { UINT32_C(3285), UINT32_C(3286) },
    { UINT32_C(3292), UINT32_C(3294) },
    { UINT32_C(3296), UINT32_C(3299) },
    { UINT32_C(3313), UINT32_C(3315) },
    { UINT32_C(3328), UINT32_C(3340) },
    { UINT32_C(3342), UINT32_C(3344) },
    { UINT32_C(3346), UINT32_C(3386) },
    { UINT32_C(3389), UINT32_C(3396) },
    { UINT32_C(3398), UINT32_C(3400) },
    { UINT32_C(3402), UINT32_C(3404) },
    { UINT32_C(3406), UINT32_C(3406) },
    { UINT32_C(3412), UINT32_C(3415) },
    { UINT32_C(3423), UINT32_C(3427) },
    { UINT32_C(3450), UINT32_C(3455) },
    { UINT32_C(3457), UINT32_C(3459) },
    { UINT32_C(3461), UINT32_C(3478) },
    { UINT32_C(3482), UINT32_C(3505) },
    { UINT32_C(3507), UINT32_C(3515) },
    { UINT32_C(3517), UINT32_C(3517) },
    { UINT32_C(3520), UINT32_C(3526) },
    { UINT32_C(3535), UINT32_C(3540) },
    { UINT32_C(3542), UINT32_C(3542) },
    { UINT32_C(3544), UINT32_C(3551) },
    { UINT32_C(3570), UINT32_C(3571) },
    { UINT32_C(3585), UINT32_C(3642) },
    { UINT32_C(3648), UINT32_C(3654) },
    { UINT32_C(3661), UINT32_C(3661) },
    { UINT32_C(3713), UINT32_C(3714) },
    { UINT32_C(3716), UINT32_C(3716) },
    { UINT32_C(3718), UINT32_C(3722) },
    { UINT32_C(3724), UINT32_C(3747) },
    { UINT32_C(3749), UINT32_C(3749) },
    { UINT32_C(3751), UINT32_C(3769) },
    { UINT32_C(3771), UINT32_C(3773) },
    { UINT32_C(3776), UINT32_C(3780) },
    { UINT32_C(3782), UINT32_C(3782) },
    { UINT32_C(3789), UINT32_C(3789) },
    { UINT32_C(3804), UINT32_C(3807) },
    { UINT32_C(3840), UINT32_C(3840) },
    { UINT32_C(3904), UINT32_C(3911) },
    { UINT32_C(3913), UINT32_C(3948) },
    { UINT32_C(3953), UINT32_C(3971) },
    { UINT32_C(3976), UINT32_C(3991) },
    { UINT32_C(3993), UINT32_C(4028) },
    { UINT32_C(4096), UINT32_C(4150) },
    { UINT32_C(4152), UINT32_C(4152) },
    { UINT32_C(4155), UINT32_C(4159) },
    { UINT32_C(4176), UINT32_C(4239) },
    { UINT32_C(4250), UINT32_C(4253) },
    { UINT32_C(4256), UINT32_C(4293) },
    { UINT32_C(4295), UINT32_C(4295) },
    { UINT32_C(4301), UINT32_C(4301) },
    { UINT32_C(4304), UINT32_C(4346) },
    { UINT32_C(4348), UINT32_C(4680) },
    { UINT32_C(4682), UINT32_C(4685) },
    { UINT32_C(4688), UINT32_C(4694) },
    { UINT32_C(4696), UINT32_C(4696) },
    { UINT32_C(4698), UINT32_C(4701) },
    { UINT32_C(4704), UINT32_C(4744) },
    { UINT32_C(4746), UINT32_C(4749) },
    { UINT32_C(4752), UINT32_C(4784) },
    { UINT32_C(4786), UINT32_C(4789) },
    { UINT32_C(4792), UINT32_C(4798) },
    { UINT32_C(4800), UINT32_C(4800) },
    { UINT32_C(4802), UINT32_C(4805) },
    { UINT32_C(4808), UINT32_C(4822) },
    { UINT32_C(4824), UINT32_C(4880) },
    { UINT32_C(4882), UINT32_C(4885) },
    { UINT32_C(4888), UINT32_C(4954) },
    { UINT32_C(4992), UINT32_C(5007) },
    { UINT32_C(5024), UINT32_C(5109) },
    { UINT32_C(5112), UINT32_C(5117) },
    { UINT32_C(5121), UINT32_C(5740) },
    { UINT32_C(5743), UINT32_C(5759) },
    { UINT32_C(5761), UINT32_C(5786) },
    { UINT32_C(5792), UINT32_C(5866) },
    { UINT32_C(5870), UINT32_C(5880) },
    { UINT32_C(5888), UINT32_C(5907) },
    { UINT32_C(5919), UINT32_C(5939) },
    { UINT32_C(5952), UINT32_C(5971) },
    { UINT32_C(5984), UINT32_C(5996) },
    { UINT32_C(5998), UINT32_C(6000) },
    { UINT32_C(6002), UINT32_C(6003) },
    { UINT32_C(6016), UINT32_C(6067) },
    { UINT32_C(6070), UINT32_C(6088) },
    { UINT32_C(6103), UINT32_C(6103) },
    { UINT32_C(6108), UINT32_C(6108) },
    { UINT32_C(6176), UINT32_C(6264) },
    { UINT32_C(6272), UINT32_C(6314) },
    { UINT32_C(6320), UINT32_C(6389) },
    { UINT32_C(6400), UINT32_C(6430) },
    { UINT32_C(6432), UINT32_C(6443) },
    { UINT32_C(6448), UINT32_C(6456) },
    { UINT32_C(6480), UINT32_C(6509) },
    { UINT32_C(6512), UINT32_C(6516) },
    { UINT32_C(6528), UINT32_C(6571) },
    { UINT32_C(6576), UINT32_C(6601) },
    { UINT32_C(6656), UINT32_C(6683) },
    { UINT32_C(6688), UINT32_C(6750) },
    { UINT32_C(6753), UINT32_C(6772) },
    { UINT32_C(6823), UINT32_C(6823) },
    { UINT32_C(6847), UINT32_C(6848) },
    { UINT32_C(6860), UINT32_C(6862) },
    { UINT32_C(6912), UINT32_C(6963) },
    { UINT32_C(6965), UINT32_C(6979) },
    { UINT32_C(6981), UINT32_C(6988) },
    { UINT32_C(7040), UINT32_C(7081) },
    { UINT32_C(7084), UINT32_C(7087) },
    { UINT32_C(7098), UINT32_C(7141) },
    { UINT32_C(7143), UINT32_C(7153) },
    { UINT32_C(7168), UINT32_C(7222) },
    { UINT32_C(7245), UINT32_C(7247) },
    { UINT32_C(7258), UINT32_C(7293) },
    { UINT32_C(7296), UINT32_C(7306) },
    { UINT32_C(7312), UINT32_C(7354) },
    { UINT32_C(7357), UINT32_C(7359) },
    { UINT32_C(7401), UINT32_C(7404) },
    { UINT32_C(7406), UINT32_C(7411) },
    { UINT32_C(7413), UINT32_C(7414) },
    { UINT32_C(7418), UINT32_C(7418) },
    { UINT32_C(7424), UINT32_C(7615) },
    { UINT32_C(7635), UINT32_C(7668) },
    { UINT32_C(7680), UINT32_C(7957) },
    { UINT32_C(7960), UINT32_C(7965) },
    { UINT32_C(7968), UINT32_C(8005) },
    { UINT32_C(8008), UINT32_C(8013) },
    { UINT32_C(8016), UINT32_C(8023) },
    { UINT32_C(8025), UINT32_C(8025) },
    { UINT32_C(8027), UINT32_C(8027) },
    { UINT32_C(8029), UINT32_C(8029) },
    { UINT32_C(8031), UINT32_C(8061) },
    { UINT32_C(8064), UINT32_C(8116) },
    { UINT32_C(8118), UINT32_C(8124) },
    { UINT32_C(8126), UINT32_C(8126) },
    { UINT32_C(8130), UINT32_C(8132) },
    { UINT32_C(8134), UINT32_C(8140) },
    { UINT32_C(8144), UINT32_C(8147) },
    { UINT32_C(8150), UINT32_C(8155) },
    { UINT32_C(8160), UINT32_C(8172) },
    { UINT32_C(8178), UINT32_C(8180) },
    { UINT32_C(8182), UINT32_C(8188) },
    { UINT32_C(8305), UINT32_C(8305) },
    { UINT32_C(8319), UINT32_C(8319) },
    { UINT32_C(8336), UINT32_C(8348) },
    { UINT32_C(8450), UINT32_C(8450) },
    { UINT32_C(8455), UINT32_C(8455) },
    { UINT32_C(8458), UINT32_C(8467) },
    { UINT32_C(8469), UINT32_C(8469) },
    { UINT32_C(8473), UINT32_C(8477) },
    { UINT32_C(8484), UINT32_C(8484) },
    { UINT32_C(8486), UINT32_C(8486) },
    { UINT32_C(8488), UINT32_C(8488) },
    { UINT32_C(8490), UINT32_C(8493) },
    { UINT32_C(8495), UINT32_C(8505) },
    { UINT32_C(8508), UINT32_C(8511) },
    { UINT32_C(8517), UINT32_C(8521) },
    { UINT32_C(8526), UINT32_C(8526) },
    { UINT32_C(8544), UINT32_C(8584) },
    { UINT32_C(9398), UINT32_C(9449) },
    { UINT32_C(11264), UINT32_C(11492) },
    { UINT32_C(11499), UINT32_C(11502) },
    { UINT32_C(11506), UINT32_C(11507) },
    { UINT32_C(11520), UINT32_C(11557) },
    { UINT32_C(11559), UINT32_C(11559) },
    { UINT32_C(11565), UINT32_C(11565) },
    { UINT32_C(11568), UINT32_C(11623) },
    { UINT32_C(11631), UINT32_C(11631) },
    { UINT32_C(11648), UINT32_C(11670) },
    { UINT32_C(11680), UINT32_C(11686) },
    { UINT32_C(11688), UINT32_C(11694) },
    { UINT32_C(11696), UINT32_C(11702) },
    { UINT32_C(11704), UINT32_C(11710) },
    { UINT32_C(11712), UINT32_C(11718) },
    { UINT32_C(11720), UINT32_C(11726) },
    { UINT32_C(11728), UINT32_C(11734) },
    { UINT32_C(11736), UINT32_C(11742) },
    { UINT32_C(11744), UINT32_C(11775) },
    { UINT32_C(11823), UINT32_C(11823) },
    { UINT32_C(12293), UINT32_C(12295) },
    { UINT32_C(12321), UINT32_C(12329) },
    { UINT32_C(12337), UINT32_C(12341) },
    { UINT32_C(12344), UINT32_C(12348) },
    { UINT32_C(12353), UINT32_C(12438) },
    { UINT32_C(12445), UINT32_C(12447) },
    { UINT32_C(12449), UINT32_C(12538) },
    { UINT32_C(12540), UINT32_C(12543) },
    { UINT32_C(12549), UINT32_C(12591) },
    { UINT32_C(12593), UINT32_C(12686) },
    { UINT32_C(12704), UINT32_C(12735) },
    { UINT32_C(12784), UINT32_C(12799) },
    { UINT32_C(13312), UINT32_C(19903) },
    { UINT32_C(19968), UINT32_C(42124) },
    { UINT32_C(42192), UINT32_C(42237) },
    { UINT32_C(42240), UINT32_C(42508) },
    { UINT32_C(42512), UINT32_C(42527) },
    { UINT32_C(42538), UINT32_C(42539) },
    { UINT32_C(42560), UINT32_C(42606) },
    { UINT32_C(42612), UINT32_C(42619) },
    { UINT32_C(42623), UINT32_C(42735) },
    { UINT32_C(42775), UINT32_C(42783) },
    { UINT32_C(42786), UINT32_C(42888) },
    { UINT32_C(42891), UINT32_C(42972) },
    { UINT32_C(42993), UINT32_C(43013) },
    { UINT32_C(43015), UINT32_C(43047) },
    { UINT32_C(43072), UINT32_C(43123) },
    { UINT32_C(43136), UINT32_C(43203) },
    { UINT32_C(43205), UINT32_C(43205) },
    { UINT32_C(43250), UINT32_C(43255) },
    { UINT32_C(43259), UINT32_C(43259) },
    { UINT32_C(43261), UINT32_C(43263) },
    { UINT32_C(43274), UINT32_C(43306) },
    { UINT32_C(43312), UINT32_C(43346) },
    { UINT32_C(43360), UINT32_C(43388) },
    { UINT32_C(43392), UINT32_C(43442) },
    { UINT32_C(43444), UINT32_C(43455) },
    { UINT32_C(43471), UINT32_C(43471) },
    { UINT32_C(43488), UINT32_C(43503) },
    { UINT32_C(43514), UINT32_C(43518) },
    { UINT32_C(43520), UINT32_C(43574) },
    { UINT32_C(43584), UINT32_C(43597) },
    { UINT32_C(43616), UINT32_C(43638) },
    { UINT32_C(43642), UINT32_C(43710) },
    { UINT32_C(43712), UINT32_C(43712) },
    { UINT32_C(43714), UINT32_C(43714) },
    { UINT32_C(43739), UINT32_C(43741) },
    { UINT32_C(43744), UINT32_C(43759) },
    { UINT32_C(43762), UINT32_C(43765) },
    { UINT32_C(43777), UINT32_C(43782) },
    { UINT32_C(43785), UINT32_C(43790) },
    { UINT32_C(43793), UINT32_C(43798) },
    { UINT32_C(43808), UINT32_C(43814) },
    { UINT32_C(43816), UINT32_C(43822) },
    { UINT32_C(43824), UINT32_C(43866) },
    { UINT32_C(43868), UINT32_C(43881) },
    { UINT32_C(43888), UINT32_C(44010) },
    { UINT32_C(44032), UINT32_C(55203) },
    { UINT32_C(55216), UINT32_C(55238) },
    { UINT32_C(55243), UINT32_C(55291) },
    { UINT32_C(63744), UINT32_C(64109) },
    { UINT32_C(64112), UINT32_C(64217) },
    { UINT32_C(64256), UINT32_C(64262) },
    { UINT32_C(64275), UINT32_C(64279) },
    { UINT32_C(64285), UINT32_C(64296) },
    { UINT32_C(64298), UINT32_C(64310) },
    { UINT32_C(64312), UINT32_C(64316) },
    { UINT32_C(64318), UINT32_C(64318) },
    { UINT32_C(64320), UINT32_C(64321) },
    { UINT32_C(64323), UINT32_C(64324) },
    { UINT32_C(64326), UINT32_C(64433) },
    { UINT32_C(64467), UINT32_C(64829) },
    { UINT32_C(64848), UINT32_C(64911) },
    { UINT32_C(64914), UINT32_C(64967) },
    { UINT32_C(65008), UINT32_C(65019) },
    { UINT32_C(65136), UINT32_C(65140) },
    { UINT32_C(65142), UINT32_C(65276) },
    { UINT32_C(65313), UINT32_C(65338) },
    { UINT32_C(65345), UINT32_C(65370) },
    { UINT32_C(65382), UINT32_C(65470) },
    { UINT32_C(65474), UINT32_C(65479) },
    { UINT32_C(65482), UINT32_C(65487) },
    { UINT32_C(65490), UINT32_C(65495) },
    { UINT32_C(65498), UINT32_C(65500) },
    { UINT32_C(65536), UINT32_C(65547) },
    { UINT32_C(65549), UINT32_C(65574) },
    { UINT32_C(65576), UINT32_C(65594) },
    { UINT32_C(65596), UINT32_C(65597) },
    { UINT32_C(65599), UINT32_C(65613) },
    { UINT32_C(65616), UINT32_C(65629) },
    { UINT32_C(65664), UINT32_C(65786) },
    { UINT32_C(65856), UINT32_C(65908) },
    { UINT32_C(66176), UINT32_C(66204) },
    { UINT32_C(66208), UINT32_C(66256) },
    { UINT32_C(66304), UINT32_C(66335) },
    { UINT32_C(66349), UINT32_C(66378) },
    { UINT32_C(66384), UINT32_C(66426) },
    { UINT32_C(66432), UINT32_C(66461) },
    { UINT32_C(66464), UINT32_C(66499) },
    { UINT32_C(66504), UINT32_C(66511) },
    { UINT32_C(66513), UINT32_C(66517) },
    { UINT32_C(66560), UINT32_C(66717) },
    { UINT32_C(66736), UINT32_C(66771) },
    { UINT32_C(66776), UINT32_C(66811) },
    { UINT32_C(66816), UINT32_C(66855) },
    { UINT32_C(66864), UINT32_C(66915) },
    { UINT32_C(66928), UINT32_C(66938) },
    { UINT32_C(66940), UINT32_C(66954) },
    { UINT32_C(66956), UINT32_C(66962) },
    { UINT32_C(66964), UINT32_C(66965) },
    { UINT32_C(66967), UINT32_C(66977) },
    { UINT32_C(66979), UINT32_C(66993) },
    { UINT32_C(66995), UINT32_C(67001) },
    { UINT32_C(67003), UINT32_C(67004) },
    { UINT32_C(67008), UINT32_C(67059) },
    { UINT32_C(67072), UINT32_C(67382) },
    { UINT32_C(67392), UINT32_C(67413) },
    { UINT32_C(67424), UINT32_C(67431) },
    { UINT32_C(67456), UINT32_C(67461) },
    { UINT32_C(67463), UINT32_C(67504) },
    { UINT32_C(67506), UINT32_C(67514) },
    { UINT32_C(67584), UINT32_C(67589) },
    { UINT32_C(67592), UINT32_C(67592) },
    { UINT32_C(67594), UINT32_C(67637) },
    { UINT32_C(67639), UINT32_C(67640) },
    { UINT32_C(67644), UINT32_C(67644) },
    { UINT32_C(67647), UINT32_C(67669) },
    { UINT32_C(67680), UINT32_C(67702) },
    { UINT32_C(67712), UINT32_C(67742) },
    { UINT32_C(67808), UINT32_C(67826) },
    { UINT32_C(67828), UINT32_C(67829) },
    { UINT32_C(67840), UINT32_C(67861) },
    { UINT32_C(67872), UINT32_C(67897) },
    { UINT32_C(67904), UINT32_C(67929) },
    { UINT32_C(67968), UINT32_C(68023) },
    { UINT32_C(68030), UINT32_C(68031) },
    { UINT32_C(68096), UINT32_C(68099) },
    { UINT32_C(68101), UINT32_C(68102) },
    { UINT32_C(68108), UINT32_C(68115) },
    { UINT32_C(68117), UINT32_C(68119) },
    { UINT32_C(68121), UINT32_C(68149) },
    { UINT32_C(68192), UINT32_C(68220) },
    { UINT32_C(68224), UINT32_C(68252) },
    { UINT32_C(68288), UINT32_C(68295) },
    { UINT32_C(68297), UINT32_C(68324) },
    { UINT32_C(68352), UINT32_C(68405) },
    { UINT32_C(68416), UINT32_C(68437) },
    { UINT32_C(68448), UINT32_C(68466) },
    { UINT32_C(68480), UINT32_C(68497) },
    { UINT32_C(68608), UINT32_C(68680) },
    { UINT32_C(68736), UINT32_C(68786) },
    { UINT32_C(68800), UINT32_C(68850) },
    { UINT32_C(68864), UINT32_C(68903) },
    { UINT32_C(68938), UINT32_C(68965) },
    { UINT32_C(68969), UINT32_C(68969) },
    { UINT32_C(68975), UINT32_C(68997) },
    { UINT32_C(69248), UINT32_C(69289) },
    { UINT32_C(69291), UINT32_C(69292) },
    { UINT32_C(69296), UINT32_C(69297) },
    { UINT32_C(69314), UINT32_C(69319) },
    { UINT32_C(69370), UINT32_C(69372) },
    { UINT32_C(69376), UINT32_C(69404) },
    { UINT32_C(69415), UINT32_C(69415) },
    { UINT32_C(69424), UINT32_C(69445) },
    { UINT32_C(69488), UINT32_C(69505) },
    { UINT32_C(69552), UINT32_C(69572) },
    { UINT32_C(69600), UINT32_C(69622) },
    { UINT32_C(69632), UINT32_C(69701) },
    { UINT32_C(69745), UINT32_C(69749) },
    { UINT32_C(69760), UINT32_C(69816) },
    { UINT32_C(69826), UINT32_C(69826) },
    { UINT32_C(69840), UINT32_C(69864) },
    { UINT32_C(69888), UINT32_C(69938) },
    { UINT32_C(69956), UINT32_C(69959) },
    { UINT32_C(69968), UINT32_C(70002) },
    { UINT32_C(70006), UINT32_C(70006) },
    { UINT32_C(70016), UINT32_C(70079) },
    { UINT32_C(70081), UINT32_C(70084) },
    { UINT32_C(70094), UINT32_C(70095) },
    { UINT32_C(70106), UINT32_C(70106) },
    { UINT32_C(70108), UINT32_C(70108) },
    { UINT32_C(70144), UINT32_C(70161) },
    { UINT32_C(70163), UINT32_C(70196) },
    { UINT32_C(70199), UINT32_C(70199) },
    { UINT32_C(70206), UINT32_C(70209) },
    { UINT32_C(70272), UINT32_C(70278) },
    { UINT32_C(70280), UINT32_C(70280) },
    { UINT32_C(70282), UINT32_C(70285) },
    { UINT32_C(70287), UINT32_C(70301) },
    { UINT32_C(70303), UINT32_C(70312) },
    { UINT32_C(70320), UINT32_C(70376) },
    { UINT32_C(70400), UINT32_C(70403) },
    { UINT32_C(70405), UINT32_C(70412) },
    { UINT32_C(70415), UINT32_C(70416) },
    { UINT32_C(70419), UINT32_C(70440) },
    { UINT32_C(70442), UINT32_C(70448) },
    { UINT32_C(70450), UINT32_C(70451) },
    { UINT32_C(70453), UINT32_C(70457) },
    { UINT32_C(70461), UINT32_C(70468) },
    { UINT32_C(70471), UINT32_C(70472) },
    { UINT32_C(70475), UINT32_C(70476) },
    { UINT32_C(70480), UINT32_C(70480) },
    { UINT32_C(70487), UINT32_C(70487) },
    { UINT32_C(70493), UINT32_C(70499) },
    { UINT32_C(70528), UINT32_C(70537) },
    { UINT32_C(70539), UINT32_C(70539) },
    { UINT32_C(70542), UINT32_C(70542) },
    { UINT32_C(70544), UINT32_C(70581) },
    { UINT32_C(70583), UINT32_C(70592) },
    { UINT32_C(70594), UINT32_C(70594) },
    { UINT32_C(70597), UINT32_C(70597) },
    { UINT32_C(70599), UINT32_C(70602) },
    { UINT32_C(70604), UINT32_C(70605) },
    { UINT32_C(70609), UINT32_C(70609) },
    { UINT32_C(70611), UINT32_C(70611) },
    { UINT32_C(70656), UINT32_C(70721) },
    { UINT32_C(70723), UINT32_C(70725) },
    { UINT32_C(70727), UINT32_C(70730) },
    { UINT32_C(70751), UINT32_C(70753) },
    { UINT32_C(70784), UINT32_C(70849) },
    { UINT32_C(70852), UINT32_C(70853) },
    { UINT32_C(70855), UINT32_C(70855) },
    { UINT32_C(71040), UINT32_C(71093) },
    { UINT32_C(71096), UINT32_C(71102) },
    { UINT32_C(71128), UINT32_C(71133) },
    { UINT32_C(71168), UINT32_C(71230) },
    { UINT32_C(71232), UINT32_C(71232) },
    { UINT32_C(71236), UINT32_C(71236) },
    { UINT32_C(71296), UINT32_C(71349) },
    { UINT32_C(71352), UINT32_C(71352) },
    { UINT32_C(71424), UINT32_C(71450) },
    { UINT32_C(71453), UINT32_C(71466) },
    { UINT32_C(71488), UINT32_C(71494) },
    { UINT32_C(71680), UINT32_C(71736) },
    { UINT32_C(71840), UINT32_C(71903) },
    { UINT32_C(71935), UINT32_C(71942) },
    { UINT32_C(71945), UINT32_C(71945) },
    { UINT32_C(71948), UINT32_C(71955) },
    { UINT32_C(71957), UINT32_C(71958) },
    { UINT32_C(71960), UINT32_C(71989) },
    { UINT32_C(71991), UINT32_C(71992) },
    { UINT32_C(71995), UINT32_C(71996) },
    { UINT32_C(71999), UINT32_C(72002) },
    { UINT32_C(72096), UINT32_C(72103) },
    { UINT32_C(72106), UINT32_C(72151) },
    { UINT32_C(72154), UINT32_C(72159) },
    { UINT32_C(72161), UINT32_C(72161) },
    { UINT32_C(72163), UINT32_C(72164) },
    { UINT32_C(72192), UINT32_C(72242) },
    { UINT32_C(72245), UINT32_C(72254) },
    { UINT32_C(72272), UINT32_C(72343) },
    { UINT32_C(72349), UINT32_C(72349) },
    { UINT32_C(72368), UINT32_C(72440) },
    { UINT32_C(72544), UINT32_C(72551) },
    { UINT32_C(72640), UINT32_C(72672) },
    { UINT32_C(72704), UINT32_C(72712) },
    { UINT32_C(72714), UINT32_C(72758) },
    { UINT32_C(72760), UINT32_C(72766) },
    { UINT32_C(72768), UINT32_C(72768) },
    { UINT32_C(72818), UINT32_C(72847) },
    { UINT32_C(72850), UINT32_C(72871) },
    { UINT32_C(72873), UINT32_C(72886) },
    { UINT32_C(72960), UINT32_C(72966) },
    { UINT32_C(72968), UINT32_C(72969) },
    { UINT32_C(72971), UINT32_C(73014) },
    { UINT32_C(73018), UINT32_C(73018) },
    { UINT32_C(73020), UINT32_C(73021) },
    { UINT32_C(73023), UINT32_C(73025) },
    { UINT32_C(73027), UINT32_C(73027) },
    { UINT32_C(73030), UINT32_C(73031) },
    { UINT32_C(73056), UINT32_C(73061) },
    { UINT32_C(73063), UINT32_C(73064) },
    { UINT32_C(73066), UINT32_C(73102) },
    { UINT32_C(73104), UINT32_C(73105) },
    { UINT32_C(73107), UINT32_C(73110) },
    { UINT32_C(73112), UINT32_C(73112) },
    { UINT32_C(73136), UINT32_C(73179) },
    { UINT32_C(73440), UINT32_C(73462) },
    { UINT32_C(73472), UINT32_C(73488) },
    { UINT32_C(73490), UINT32_C(73530) },
    { UINT32_C(73534), UINT32_C(73536) },
    { UINT32_C(73648), UINT32_C(73648) },
    { UINT32_C(73728), UINT32_C(74649) },
    { UINT32_C(74752), UINT32_C(74862) },
    { UINT32_C(74880), UINT32_C(75075) },
    { UINT32_C(77712), UINT32_C(77808) },
    { UINT32_C(77824), UINT32_C(78895) },
    { UINT32_C(78913), UINT32_C(78918) },
    { UINT32_C(78944), UINT32_C(82938) },
    { UINT32_C(82944), UINT32_C(83526) },
    { UINT32_C(90368), UINT32_C(90414) },
    { UINT32_C(92160), UINT32_C(92728) },
    { UINT32_C(92736), UINT32_C(92766) },
    { UINT32_C(92784), UINT32_C(92862) },
    { UINT32_C(92880), UINT32_C(92909) },
    { UINT32_C(92928), UINT32_C(92975) },
    { UINT32_C(92992), UINT32_C(92995) },
    { UINT32_C(93027), UINT32_C(93047) },
    { UINT32_C(93053), UINT32_C(93071) },
    { UINT32_C(93504), UINT32_C(93548) },
    { UINT32_C(93760), UINT32_C(93823) },
    { UINT32_C(93856), UINT32_C(93880) },
    { UINT32_C(93883), UINT32_C(93907) },
    { UINT32_C(93952), UINT32_C(94026) },
    { UINT32_C(94031), UINT32_C(94087) },
    { UINT32_C(94095), UINT32_C(94111) },
    { UINT32_C(94176), UINT32_C(94177) },
    { UINT32_C(94179), UINT32_C(94179) },
    { UINT32_C(94192), UINT32_C(94198) },
    { UINT32_C(94208), UINT32_C(101589) },
    { UINT32_C(101631), UINT32_C(101662) },
    { UINT32_C(101760), UINT32_C(101874) },
    { UINT32_C(110576), UINT32_C(110579) },
    { UINT32_C(110581), UINT32_C(110587) },
    { UINT32_C(110589), UINT32_C(110590) },
    { UINT32_C(110592), UINT32_C(110882) },
    { UINT32_C(110898), UINT32_C(110898) },
    { UINT32_C(110928), UINT32_C(110930) },
    { UINT32_C(110933), UINT32_C(110933) },
    { UINT32_C(110948), UINT32_C(110951) },
    { UINT32_C(110960), UINT32_C(111355) },
    { UINT32_C(113664), UINT32_C(113770) },
    { UINT32_C(113776), UINT32_C(113788) },
    { UINT32_C(113792), UINT32_C(113800) },
    { UINT32_C(113808), UINT32_C(113817) },
    { UINT32_C(113822), UINT32_C(113822) },
    { UINT32_C(119808), UINT32_C(119892) },
    { UINT32_C(119894), UINT32_C(119964) },
    { UINT32_C(119966), UINT32_C(119967) },
    { UINT32_C(119970), UINT32_C(119970) },
    { UINT32_C(119973), UINT32_C(119974) },
    { UINT32_C(119977), UINT32_C(119980) },
    { UINT32_C(119982), UINT32_C(119993) },
    { UINT32_C(119995), UINT32_C(119995) },
    { UINT32_C(119997), UINT32_C(120003) },
    { UINT32_C(120005), UINT32_C(120069) },
    { UINT32_C(120071), UINT32_C(120074) },
    { UINT32_C(120077), UINT32_C(120084) },
    { UINT32_C(120086), UINT32_C(120092) },
    { UINT32_C(120094), UINT32_C(120121) },
    { UINT32_C(120123), UINT32_C(120126) },
    { UINT32_C(120128), UINT32_C(120132) },
    { UINT32_C(120134), UINT32_C(120134) },
    { UINT32_C(120138), UINT32_C(120144) },
    { UINT32_C(120146), UINT32_C(120485) },
    { UINT32_C(120488), UINT32_C(120512) },
    { UINT32_C(120514), UINT32_C(120538) },
    { UINT32_C(120540), UINT32_C(120570) },
    { UINT32_C(120572), UINT32_C(120596) },
    { UINT32_C(120598), UINT32_C(120628) },
    { UINT32_C(120630), UINT32_C(120654) },
    { UINT32_C(120656), UINT32_C(120686) },
    { UINT32_C(120688), UINT32_C(120712) },
    { UINT32_C(120714), UINT32_C(120744) },
    { UINT32_C(120746), UINT32_C(120770) },
    { UINT32_C(120772), UINT32_C(120779) },
    { UINT32_C(122624), UINT32_C(122654) },
    { UINT32_C(122661), UINT32_C(122666) },
    { UINT32_C(122880), UINT32_C(122886) },
    { UINT32_C(122888), UINT32_C(122904) },
    { UINT32_C(122907), UINT32_C(122913) },
    { UINT32_C(122915), UINT32_C(122916) },
    { UINT32_C(122918), UINT32_C(122922) },
    { UINT32_C(122928), UINT32_C(122989) },
    { UINT32_C(123023), UINT32_C(123023) },
    { UINT32_C(123136), UINT32_C(123180) },
    { UINT32_C(123191), UINT32_C(123197) },
    { UINT32_C(123214), UINT32_C(123214) },
    { UINT32_C(123536), UINT32_C(123565) },
    { UINT32_C(123584), UINT32_C(123627) },
    { UINT32_C(124112), UINT32_C(124139) },
    { UINT32_C(124368), UINT32_C(124397) },
    { UINT32_C(124400), UINT32_C(124400) },
    { UINT32_C(124608), UINT32_C(124638) },
    { UINT32_C(124640), UINT32_C(124661) },
    { UINT32_C(124670), UINT32_C(124671) },
    { UINT32_C(124896), UINT32_C(124902) },
    { UINT32_C(124904), UINT32_C(124907) },
    { UINT32_C(124909), UINT32_C(124910) },
    { UINT32_C(124912), UINT32_C(124926) },
    { UINT32_C(124928), UINT32_C(125124) },
    { UINT32_C(125184), UINT32_C(125251) },
    { UINT32_C(125255), UINT32_C(125255) },
    { UINT32_C(125259), UINT32_C(125259) },
    { UINT32_C(126464), UINT32_C(126467) },
    { UINT32_C(126469), UINT32_C(126495) },
    { UINT32_C(126497), UINT32_C(126498) },
    { UINT32_C(126500), UINT32_C(126500) },
    { UINT32_C(126503), UINT32_C(126503) },
    { UINT32_C(126505), UINT32_C(126514) },
    { UINT32_C(126516), UINT32_C(126519) },
    { UINT32_C(126521), UINT32_C(126521) },
    { UINT32_C(126523), UINT32_C(126523) },
    { UINT32_C(126530), UINT32_C(126530) },
    { UINT32_C(126535), UINT32_C(126535) },
    { UINT32_C(126537), UINT32_C(126537) },
    { UINT32_C(126539), UINT32_C(126539) },
    { UINT32_C(126541), UINT32_C(126543) },
    { UINT32_C(126545), UINT32_C(126546) },
    { UINT32_C(126548), UINT32_C(126548) },
    { UINT32_C(126551), UINT32_C(126551) },
    { UINT32_C(126553), UINT32_C(126553) },
    { UINT32_C(126555), UINT32_C(126555) },
    { UINT32_C(126557), UINT32_C(126557) },
    { UINT32_C(126559), UINT32_C(126559) },
    { UINT32_C(126561), UINT32_C(126562) },
    { UINT32_C(126564), UINT32_C(126564) },
    { UINT32_C(126567), UINT32_C(126570) },
    { UINT32_C(126572), UINT32_C(126578) },
    { UINT32_C(126580), UINT32_C(126583) },
    { UINT32_C(126585), UINT32_C(126588) },
    { UINT32_C(126590), UINT32_C(126590) },
    { UINT32_C(126592), UINT32_C(126601) },
    { UINT32_C(126603), UINT32_C(126619) },
    { UINT32_C(126625), UINT32_C(126627) },
    { UINT32_C(126629), UINT32_C(126633) },
    { UINT32_C(126635), UINT32_C(126651) },
    { UINT32_C(127280), UINT32_C(127305) },
    { UINT32_C(127312), UINT32_C(127337) },
    { UINT32_C(127344), UINT32_C(127369) },
    { UINT32_C(131072), UINT32_C(173791) },
    { UINT32_C(173824), UINT32_C(178205) },
    { UINT32_C(178208), UINT32_C(183981) },
    { UINT32_C(183984), UINT32_C(191456) },
    { UINT32_C(191472), UINT32_C(192093) },
    { UINT32_C(194560), UINT32_C(195101) },
    { UINT32_C(196608), UINT32_C(201546) },
    { UINT32_C(201552), UINT32_C(210041) },
};
static const MimiUnicodeRange MIMI_UNICODE_ALPHANUMERIC[] = {
    { UINT32_C(48), UINT32_C(57) },
    { UINT32_C(65), UINT32_C(90) },
    { UINT32_C(97), UINT32_C(122) },
    { UINT32_C(170), UINT32_C(170) },
    { UINT32_C(178), UINT32_C(179) },
    { UINT32_C(181), UINT32_C(181) },
    { UINT32_C(185), UINT32_C(186) },
    { UINT32_C(188), UINT32_C(190) },
    { UINT32_C(192), UINT32_C(214) },
    { UINT32_C(216), UINT32_C(246) },
    { UINT32_C(248), UINT32_C(705) },
    { UINT32_C(710), UINT32_C(721) },
    { UINT32_C(736), UINT32_C(740) },
    { UINT32_C(748), UINT32_C(748) },
    { UINT32_C(750), UINT32_C(750) },
    { UINT32_C(837), UINT32_C(837) },
    { UINT32_C(867), UINT32_C(884) },
    { UINT32_C(886), UINT32_C(887) },
    { UINT32_C(890), UINT32_C(893) },
    { UINT32_C(895), UINT32_C(895) },
    { UINT32_C(902), UINT32_C(902) },
    { UINT32_C(904), UINT32_C(906) },
    { UINT32_C(908), UINT32_C(908) },
    { UINT32_C(910), UINT32_C(929) },
    { UINT32_C(931), UINT32_C(1013) },
    { UINT32_C(1015), UINT32_C(1153) },
    { UINT32_C(1162), UINT32_C(1327) },
    { UINT32_C(1329), UINT32_C(1366) },
    { UINT32_C(1369), UINT32_C(1369) },
    { UINT32_C(1376), UINT32_C(1416) },
    { UINT32_C(1456), UINT32_C(1469) },
    { UINT32_C(1471), UINT32_C(1471) },
    { UINT32_C(1473), UINT32_C(1474) },
    { UINT32_C(1476), UINT32_C(1477) },
    { UINT32_C(1479), UINT32_C(1479) },
    { UINT32_C(1488), UINT32_C(1514) },
    { UINT32_C(1519), UINT32_C(1522) },
    { UINT32_C(1552), UINT32_C(1562) },
    { UINT32_C(1568), UINT32_C(1623) },
    { UINT32_C(1625), UINT32_C(1641) },
    { UINT32_C(1646), UINT32_C(1747) },
    { UINT32_C(1749), UINT32_C(1756) },
    { UINT32_C(1761), UINT32_C(1768) },
    { UINT32_C(1773), UINT32_C(1788) },
    { UINT32_C(1791), UINT32_C(1791) },
    { UINT32_C(1808), UINT32_C(1855) },
    { UINT32_C(1869), UINT32_C(1969) },
    { UINT32_C(1984), UINT32_C(2026) },
    { UINT32_C(2036), UINT32_C(2037) },
    { UINT32_C(2042), UINT32_C(2042) },
    { UINT32_C(2048), UINT32_C(2071) },
    { UINT32_C(2074), UINT32_C(2092) },
    { UINT32_C(2112), UINT32_C(2136) },
    { UINT32_C(2144), UINT32_C(2154) },
    { UINT32_C(2160), UINT32_C(2183) },
    { UINT32_C(2185), UINT32_C(2191) },
    { UINT32_C(2199), UINT32_C(2199) },
    { UINT32_C(2208), UINT32_C(2249) },
    { UINT32_C(2260), UINT32_C(2271) },
    { UINT32_C(2275), UINT32_C(2281) },
    { UINT32_C(2288), UINT32_C(2363) },
    { UINT32_C(2365), UINT32_C(2380) },
    { UINT32_C(2382), UINT32_C(2384) },
    { UINT32_C(2389), UINT32_C(2403) },
    { UINT32_C(2406), UINT32_C(2415) },
    { UINT32_C(2417), UINT32_C(2435) },
    { UINT32_C(2437), UINT32_C(2444) },
    { UINT32_C(2447), UINT32_C(2448) },
    { UINT32_C(2451), UINT32_C(2472) },
    { UINT32_C(2474), UINT32_C(2480) },
    { UINT32_C(2482), UINT32_C(2482) },
    { UINT32_C(2486), UINT32_C(2489) },
    { UINT32_C(2493), UINT32_C(2500) },
    { UINT32_C(2503), UINT32_C(2504) },
    { UINT32_C(2507), UINT32_C(2508) },
    { UINT32_C(2510), UINT32_C(2510) },
    { UINT32_C(2519), UINT32_C(2519) },
    { UINT32_C(2524), UINT32_C(2525) },
    { UINT32_C(2527), UINT32_C(2531) },
    { UINT32_C(2534), UINT32_C(2545) },
    { UINT32_C(2548), UINT32_C(2553) },
    { UINT32_C(2556), UINT32_C(2556) },
    { UINT32_C(2561), UINT32_C(2563) },
    { UINT32_C(2565), UINT32_C(2570) },
    { UINT32_C(2575), UINT32_C(2576) },
    { UINT32_C(2579), UINT32_C(2600) },
    { UINT32_C(2602), UINT32_C(2608) },
    { UINT32_C(2610), UINT32_C(2611) },
    { UINT32_C(2613), UINT32_C(2614) },
    { UINT32_C(2616), UINT32_C(2617) },
    { UINT32_C(2622), UINT32_C(2626) },
    { UINT32_C(2631), UINT32_C(2632) },
    { UINT32_C(2635), UINT32_C(2636) },
    { UINT32_C(2641), UINT32_C(2641) },
    { UINT32_C(2649), UINT32_C(2652) },
    { UINT32_C(2654), UINT32_C(2654) },
    { UINT32_C(2662), UINT32_C(2677) },
    { UINT32_C(2689), UINT32_C(2691) },
    { UINT32_C(2693), UINT32_C(2701) },
    { UINT32_C(2703), UINT32_C(2705) },
    { UINT32_C(2707), UINT32_C(2728) },
    { UINT32_C(2730), UINT32_C(2736) },
    { UINT32_C(2738), UINT32_C(2739) },
    { UINT32_C(2741), UINT32_C(2745) },
    { UINT32_C(2749), UINT32_C(2757) },
    { UINT32_C(2759), UINT32_C(2761) },
    { UINT32_C(2763), UINT32_C(2764) },
    { UINT32_C(2768), UINT32_C(2768) },
    { UINT32_C(2784), UINT32_C(2787) },
    { UINT32_C(2790), UINT32_C(2799) },
    { UINT32_C(2809), UINT32_C(2812) },
    { UINT32_C(2817), UINT32_C(2819) },
    { UINT32_C(2821), UINT32_C(2828) },
    { UINT32_C(2831), UINT32_C(2832) },
    { UINT32_C(2835), UINT32_C(2856) },
    { UINT32_C(2858), UINT32_C(2864) },
    { UINT32_C(2866), UINT32_C(2867) },
    { UINT32_C(2869), UINT32_C(2873) },
    { UINT32_C(2877), UINT32_C(2884) },
    { UINT32_C(2887), UINT32_C(2888) },
    { UINT32_C(2891), UINT32_C(2892) },
    { UINT32_C(2902), UINT32_C(2903) },
    { UINT32_C(2908), UINT32_C(2909) },
    { UINT32_C(2911), UINT32_C(2915) },
    { UINT32_C(2918), UINT32_C(2927) },
    { UINT32_C(2929), UINT32_C(2935) },
    { UINT32_C(2946), UINT32_C(2947) },
    { UINT32_C(2949), UINT32_C(2954) },
    { UINT32_C(2958), UINT32_C(2960) },
    { UINT32_C(2962), UINT32_C(2965) },
    { UINT32_C(2969), UINT32_C(2970) },
    { UINT32_C(2972), UINT32_C(2972) },
    { UINT32_C(2974), UINT32_C(2975) },
    { UINT32_C(2979), UINT32_C(2980) },
    { UINT32_C(2984), UINT32_C(2986) },
    { UINT32_C(2990), UINT32_C(3001) },
    { UINT32_C(3006), UINT32_C(3010) },
    { UINT32_C(3014), UINT32_C(3016) },
    { UINT32_C(3018), UINT32_C(3020) },
    { UINT32_C(3024), UINT32_C(3024) },
    { UINT32_C(3031), UINT32_C(3031) },
    { UINT32_C(3046), UINT32_C(3058) },
    { UINT32_C(3072), UINT32_C(3084) },
    { UINT32_C(3086), UINT32_C(3088) },
    { UINT32_C(3090), UINT32_C(3112) },
    { UINT32_C(3114), UINT32_C(3129) },
    { UINT32_C(3133), UINT32_C(3140) },
    { UINT32_C(3142), UINT32_C(3144) },
    { UINT32_C(3146), UINT32_C(3148) },
    { UINT32_C(3157), UINT32_C(3158) },
    { UINT32_C(3160), UINT32_C(3162) },
    { UINT32_C(3164), UINT32_C(3165) },
    { UINT32_C(3168), UINT32_C(3171) },
    { UINT32_C(3174), UINT32_C(3183) },
    { UINT32_C(3192), UINT32_C(3198) },
    { UINT32_C(3200), UINT32_C(3203) },
    { UINT32_C(3205), UINT32_C(3212) },
    { UINT32_C(3214), UINT32_C(3216) },
    { UINT32_C(3218), UINT32_C(3240) },
    { UINT32_C(3242), UINT32_C(3251) },
    { UINT32_C(3253), UINT32_C(3257) },
    { UINT32_C(3261), UINT32_C(3268) },
    { UINT32_C(3270), UINT32_C(3272) },
    { UINT32_C(3274), UINT32_C(3276) },
    { UINT32_C(3285), UINT32_C(3286) },
    { UINT32_C(3292), UINT32_C(3294) },
    { UINT32_C(3296), UINT32_C(3299) },
    { UINT32_C(3302), UINT32_C(3311) },
    { UINT32_C(3313), UINT32_C(3315) },
    { UINT32_C(3328), UINT32_C(3340) },
    { UINT32_C(3342), UINT32_C(3344) },
    { UINT32_C(3346), UINT32_C(3386) },
    { UINT32_C(3389), UINT32_C(3396) },
    { UINT32_C(3398), UINT32_C(3400) },
    { UINT32_C(3402), UINT32_C(3404) },
    { UINT32_C(3406), UINT32_C(3406) },
    { UINT32_C(3412), UINT32_C(3427) },
    { UINT32_C(3430), UINT32_C(3448) },
    { UINT32_C(3450), UINT32_C(3455) },
    { UINT32_C(3457), UINT32_C(3459) },
    { UINT32_C(3461), UINT32_C(3478) },
    { UINT32_C(3482), UINT32_C(3505) },
    { UINT32_C(3507), UINT32_C(3515) },
    { UINT32_C(3517), UINT32_C(3517) },
    { UINT32_C(3520), UINT32_C(3526) },
    { UINT32_C(3535), UINT32_C(3540) },
    { UINT32_C(3542), UINT32_C(3542) },
    { UINT32_C(3544), UINT32_C(3551) },
    { UINT32_C(3558), UINT32_C(3567) },
    { UINT32_C(3570), UINT32_C(3571) },
    { UINT32_C(3585), UINT32_C(3642) },
    { UINT32_C(3648), UINT32_C(3654) },
    { UINT32_C(3661), UINT32_C(3661) },
    { UINT32_C(3664), UINT32_C(3673) },
    { UINT32_C(3713), UINT32_C(3714) },
    { UINT32_C(3716), UINT32_C(3716) },
    { UINT32_C(3718), UINT32_C(3722) },
    { UINT32_C(3724), UINT32_C(3747) },
    { UINT32_C(3749), UINT32_C(3749) },
    { UINT32_C(3751), UINT32_C(3769) },
    { UINT32_C(3771), UINT32_C(3773) },
    { UINT32_C(3776), UINT32_C(3780) },
    { UINT32_C(3782), UINT32_C(3782) },
    { UINT32_C(3789), UINT32_C(3789) },
    { UINT32_C(3792), UINT32_C(3801) },
    { UINT32_C(3804), UINT32_C(3807) },
    { UINT32_C(3840), UINT32_C(3840) },
    { UINT32_C(3872), UINT32_C(3891) },
    { UINT32_C(3904), UINT32_C(3911) },
    { UINT32_C(3913), UINT32_C(3948) },
    { UINT32_C(3953), UINT32_C(3971) },
    { UINT32_C(3976), UINT32_C(3991) },
    { UINT32_C(3993), UINT32_C(4028) },
    { UINT32_C(4096), UINT32_C(4150) },
    { UINT32_C(4152), UINT32_C(4152) },
    { UINT32_C(4155), UINT32_C(4169) },
    { UINT32_C(4176), UINT32_C(4253) },
    { UINT32_C(4256), UINT32_C(4293) },
    { UINT32_C(4295), UINT32_C(4295) },
    { UINT32_C(4301), UINT32_C(4301) },
    { UINT32_C(4304), UINT32_C(4346) },
    { UINT32_C(4348), UINT32_C(4680) },
    { UINT32_C(4682), UINT32_C(4685) },
    { UINT32_C(4688), UINT32_C(4694) },
    { UINT32_C(4696), UINT32_C(4696) },
    { UINT32_C(4698), UINT32_C(4701) },
    { UINT32_C(4704), UINT32_C(4744) },
    { UINT32_C(4746), UINT32_C(4749) },
    { UINT32_C(4752), UINT32_C(4784) },
    { UINT32_C(4786), UINT32_C(4789) },
    { UINT32_C(4792), UINT32_C(4798) },
    { UINT32_C(4800), UINT32_C(4800) },
    { UINT32_C(4802), UINT32_C(4805) },
    { UINT32_C(4808), UINT32_C(4822) },
    { UINT32_C(4824), UINT32_C(4880) },
    { UINT32_C(4882), UINT32_C(4885) },
    { UINT32_C(4888), UINT32_C(4954) },
    { UINT32_C(4969), UINT32_C(4988) },
    { UINT32_C(4992), UINT32_C(5007) },
    { UINT32_C(5024), UINT32_C(5109) },
    { UINT32_C(5112), UINT32_C(5117) },
    { UINT32_C(5121), UINT32_C(5740) },
    { UINT32_C(5743), UINT32_C(5759) },
    { UINT32_C(5761), UINT32_C(5786) },
    { UINT32_C(5792), UINT32_C(5866) },
    { UINT32_C(5870), UINT32_C(5880) },
    { UINT32_C(5888), UINT32_C(5907) },
    { UINT32_C(5919), UINT32_C(5939) },
    { UINT32_C(5952), UINT32_C(5971) },
    { UINT32_C(5984), UINT32_C(5996) },
    { UINT32_C(5998), UINT32_C(6000) },
    { UINT32_C(6002), UINT32_C(6003) },
    { UINT32_C(6016), UINT32_C(6067) },
    { UINT32_C(6070), UINT32_C(6088) },
    { UINT32_C(6103), UINT32_C(6103) },
    { UINT32_C(6108), UINT32_C(6108) },
    { UINT32_C(6112), UINT32_C(6121) },
    { UINT32_C(6128), UINT32_C(6137) },
    { UINT32_C(6160), UINT32_C(6169) },
    { UINT32_C(6176), UINT32_C(6264) },
    { UINT32_C(6272), UINT32_C(6314) },
    { UINT32_C(6320), UINT32_C(6389) },
    { UINT32_C(6400), UINT32_C(6430) },
    { UINT32_C(6432), UINT32_C(6443) },
    { UINT32_C(6448), UINT32_C(6456) },
    { UINT32_C(6470), UINT32_C(6509) },
    { UINT32_C(6512), UINT32_C(6516) },
    { UINT32_C(6528), UINT32_C(6571) },
    { UINT32_C(6576), UINT32_C(6601) },
    { UINT32_C(6608), UINT32_C(6618) },
    { UINT32_C(6656), UINT32_C(6683) },
    { UINT32_C(6688), UINT32_C(6750) },
    { UINT32_C(6753), UINT32_C(6772) },
    { UINT32_C(6784), UINT32_C(6793) },
    { UINT32_C(6800), UINT32_C(6809) },
    { UINT32_C(6823), UINT32_C(6823) },
    { UINT32_C(6847), UINT32_C(6848) },
    { UINT32_C(6860), UINT32_C(6862) },
    { UINT32_C(6912), UINT32_C(6963) },
    { UINT32_C(6965), UINT32_C(6979) },
    { UINT32_C(6981), UINT32_C(6988) },
    { UINT32_C(6992), UINT32_C(7001) },
    { UINT32_C(7040), UINT32_C(7081) },
    { UINT32_C(7084), UINT32_C(7141) },
    { UINT32_C(7143), UINT32_C(7153) },
    { UINT32_C(7168), UINT32_C(7222) },
    { UINT32_C(7232), UINT32_C(7241) },
    { UINT32_C(7245), UINT32_C(7293) },
    { UINT32_C(7296), UINT32_C(7306) },
    { UINT32_C(7312), UINT32_C(7354) },
    { UINT32_C(7357), UINT32_C(7359) },
    { UINT32_C(7401), UINT32_C(7404) },
    { UINT32_C(7406), UINT32_C(7411) },
    { UINT32_C(7413), UINT32_C(7414) },
    { UINT32_C(7418), UINT32_C(7418) },
    { UINT32_C(7424), UINT32_C(7615) },
    { UINT32_C(7635), UINT32_C(7668) },
    { UINT32_C(7680), UINT32_C(7957) },
    { UINT32_C(7960), UINT32_C(7965) },
    { UINT32_C(7968), UINT32_C(8005) },
    { UINT32_C(8008), UINT32_C(8013) },
    { UINT32_C(8016), UINT32_C(8023) },
    { UINT32_C(8025), UINT32_C(8025) },
    { UINT32_C(8027), UINT32_C(8027) },
    { UINT32_C(8029), UINT32_C(8029) },
    { UINT32_C(8031), UINT32_C(8061) },
    { UINT32_C(8064), UINT32_C(8116) },
    { UINT32_C(8118), UINT32_C(8124) },
    { UINT32_C(8126), UINT32_C(8126) },
    { UINT32_C(8130), UINT32_C(8132) },
    { UINT32_C(8134), UINT32_C(8140) },
    { UINT32_C(8144), UINT32_C(8147) },
    { UINT32_C(8150), UINT32_C(8155) },
    { UINT32_C(8160), UINT32_C(8172) },
    { UINT32_C(8178), UINT32_C(8180) },
    { UINT32_C(8182), UINT32_C(8188) },
    { UINT32_C(8304), UINT32_C(8305) },
    { UINT32_C(8308), UINT32_C(8313) },
    { UINT32_C(8319), UINT32_C(8329) },
    { UINT32_C(8336), UINT32_C(8348) },
    { UINT32_C(8450), UINT32_C(8450) },
    { UINT32_C(8455), UINT32_C(8455) },
    { UINT32_C(8458), UINT32_C(8467) },
    { UINT32_C(8469), UINT32_C(8469) },
    { UINT32_C(8473), UINT32_C(8477) },
    { UINT32_C(8484), UINT32_C(8484) },
    { UINT32_C(8486), UINT32_C(8486) },
    { UINT32_C(8488), UINT32_C(8488) },
    { UINT32_C(8490), UINT32_C(8493) },
    { UINT32_C(8495), UINT32_C(8505) },
    { UINT32_C(8508), UINT32_C(8511) },
    { UINT32_C(8517), UINT32_C(8521) },
    { UINT32_C(8526), UINT32_C(8526) },
    { UINT32_C(8528), UINT32_C(8585) },
    { UINT32_C(9312), UINT32_C(9371) },
    { UINT32_C(9398), UINT32_C(9471) },
    { UINT32_C(10102), UINT32_C(10131) },
    { UINT32_C(11264), UINT32_C(11492) },
    { UINT32_C(11499), UINT32_C(11502) },
    { UINT32_C(11506), UINT32_C(11507) },
    { UINT32_C(11517), UINT32_C(11517) },
    { UINT32_C(11520), UINT32_C(11557) },
    { UINT32_C(11559), UINT32_C(11559) },
    { UINT32_C(11565), UINT32_C(11565) },
    { UINT32_C(11568), UINT32_C(11623) },
    { UINT32_C(11631), UINT32_C(11631) },
    { UINT32_C(11648), UINT32_C(11670) },
    { UINT32_C(11680), UINT32_C(11686) },
    { UINT32_C(11688), UINT32_C(11694) },
    { UINT32_C(11696), UINT32_C(11702) },
    { UINT32_C(11704), UINT32_C(11710) },
    { UINT32_C(11712), UINT32_C(11718) },
    { UINT32_C(11720), UINT32_C(11726) },
    { UINT32_C(11728), UINT32_C(11734) },
    { UINT32_C(11736), UINT32_C(11742) },
    { UINT32_C(11744), UINT32_C(11775) },
    { UINT32_C(11823), UINT32_C(11823) },
    { UINT32_C(12293), UINT32_C(12295) },
    { UINT32_C(12321), UINT32_C(12329) },
    { UINT32_C(12337), UINT32_C(12341) },
    { UINT32_C(12344), UINT32_C(12348) },
    { UINT32_C(12353), UINT32_C(12438) },
    { UINT32_C(12445), UINT32_C(12447) },
    { UINT32_C(12449), UINT32_C(12538) },
    { UINT32_C(12540), UINT32_C(12543) },
    { UINT32_C(12549), UINT32_C(12591) },
    { UINT32_C(12593), UINT32_C(12686) },
    { UINT32_C(12690), UINT32_C(12693) },
    { UINT32_C(12704), UINT32_C(12735) },
    { UINT32_C(12784), UINT32_C(12799) },
    { UINT32_C(12832), UINT32_C(12841) },
    { UINT32_C(12872), UINT32_C(12879) },
    { UINT32_C(12881), UINT32_C(12895) },
    { UINT32_C(12928), UINT32_C(12937) },
    { UINT32_C(12977), UINT32_C(12991) },
    { UINT32_C(13312), UINT32_C(19903) },
    { UINT32_C(19968), UINT32_C(42124) },
    { UINT32_C(42192), UINT32_C(42237) },
    { UINT32_C(42240), UINT32_C(42508) },
    { UINT32_C(42512), UINT32_C(42539) },
    { UINT32_C(42560), UINT32_C(42606) },
    { UINT32_C(42612), UINT32_C(42619) },
    { UINT32_C(42623), UINT32_C(42735) },
    { UINT32_C(42775), UINT32_C(42783) },
    { UINT32_C(42786), UINT32_C(42888) },
    { UINT32_C(42891), UINT32_C(42972) },
    { UINT32_C(42993), UINT32_C(43013) },
    { UINT32_C(43015), UINT32_C(43047) },
    { UINT32_C(43056), UINT32_C(43061) },
    { UINT32_C(43072), UINT32_C(43123) },
    { UINT32_C(43136), UINT32_C(43203) },
    { UINT32_C(43205), UINT32_C(43205) },
    { UINT32_C(43216), UINT32_C(43225) },
    { UINT32_C(43250), UINT32_C(43255) },
    { UINT32_C(43259), UINT32_C(43259) },
    { UINT32_C(43261), UINT32_C(43306) },
    { UINT32_C(43312), UINT32_C(43346) },
    { UINT32_C(43360), UINT32_C(43388) },
    { UINT32_C(43392), UINT32_C(43442) },
    { UINT32_C(43444), UINT32_C(43455) },
    { UINT32_C(43471), UINT32_C(43481) },
    { UINT32_C(43488), UINT32_C(43518) },
    { UINT32_C(43520), UINT32_C(43574) },
    { UINT32_C(43584), UINT32_C(43597) },
    { UINT32_C(43600), UINT32_C(43609) },
    { UINT32_C(43616), UINT32_C(43638) },
    { UINT32_C(43642), UINT32_C(43710) },
    { UINT32_C(43712), UINT32_C(43712) },
    { UINT32_C(43714), UINT32_C(43714) },
    { UINT32_C(43739), UINT32_C(43741) },
    { UINT32_C(43744), UINT32_C(43759) },
    { UINT32_C(43762), UINT32_C(43765) },
    { UINT32_C(43777), UINT32_C(43782) },
    { UINT32_C(43785), UINT32_C(43790) },
    { UINT32_C(43793), UINT32_C(43798) },
    { UINT32_C(43808), UINT32_C(43814) },
    { UINT32_C(43816), UINT32_C(43822) },
    { UINT32_C(43824), UINT32_C(43866) },
    { UINT32_C(43868), UINT32_C(43881) },
    { UINT32_C(43888), UINT32_C(44010) },
    { UINT32_C(44016), UINT32_C(44025) },
    { UINT32_C(44032), UINT32_C(55203) },
    { UINT32_C(55216), UINT32_C(55238) },
    { UINT32_C(55243), UINT32_C(55291) },
    { UINT32_C(63744), UINT32_C(64109) },
    { UINT32_C(64112), UINT32_C(64217) },
    { UINT32_C(64256), UINT32_C(64262) },
    { UINT32_C(64275), UINT32_C(64279) },
    { UINT32_C(64285), UINT32_C(64296) },
    { UINT32_C(64298), UINT32_C(64310) },
    { UINT32_C(64312), UINT32_C(64316) },
    { UINT32_C(64318), UINT32_C(64318) },
    { UINT32_C(64320), UINT32_C(64321) },
    { UINT32_C(64323), UINT32_C(64324) },
    { UINT32_C(64326), UINT32_C(64433) },
    { UINT32_C(64467), UINT32_C(64829) },
    { UINT32_C(64848), UINT32_C(64911) },
    { UINT32_C(64914), UINT32_C(64967) },
    { UINT32_C(65008), UINT32_C(65019) },
    { UINT32_C(65136), UINT32_C(65140) },
    { UINT32_C(65142), UINT32_C(65276) },
    { UINT32_C(65296), UINT32_C(65305) },
    { UINT32_C(65313), UINT32_C(65338) },
    { UINT32_C(65345), UINT32_C(65370) },
    { UINT32_C(65382), UINT32_C(65470) },
    { UINT32_C(65474), UINT32_C(65479) },
    { UINT32_C(65482), UINT32_C(65487) },
    { UINT32_C(65490), UINT32_C(65495) },
    { UINT32_C(65498), UINT32_C(65500) },
    { UINT32_C(65536), UINT32_C(65547) },
    { UINT32_C(65549), UINT32_C(65574) },
    { UINT32_C(65576), UINT32_C(65594) },
    { UINT32_C(65596), UINT32_C(65597) },
    { UINT32_C(65599), UINT32_C(65613) },
    { UINT32_C(65616), UINT32_C(65629) },
    { UINT32_C(65664), UINT32_C(65786) },
    { UINT32_C(65799), UINT32_C(65843) },
    { UINT32_C(65856), UINT32_C(65912) },
    { UINT32_C(65930), UINT32_C(65931) },
    { UINT32_C(66176), UINT32_C(66204) },
    { UINT32_C(66208), UINT32_C(66256) },
    { UINT32_C(66273), UINT32_C(66299) },
    { UINT32_C(66304), UINT32_C(66339) },
    { UINT32_C(66349), UINT32_C(66378) },
    { UINT32_C(66384), UINT32_C(66426) },
    { UINT32_C(66432), UINT32_C(66461) },
    { UINT32_C(66464), UINT32_C(66499) },
    { UINT32_C(66504), UINT32_C(66511) },
    { UINT32_C(66513), UINT32_C(66517) },
    { UINT32_C(66560), UINT32_C(66717) },
    { UINT32_C(66720), UINT32_C(66729) },
    { UINT32_C(66736), UINT32_C(66771) },
    { UINT32_C(66776), UINT32_C(66811) },
    { UINT32_C(66816), UINT32_C(66855) },
    { UINT32_C(66864), UINT32_C(66915) },
    { UINT32_C(66928), UINT32_C(66938) },
    { UINT32_C(66940), UINT32_C(66954) },
    { UINT32_C(66956), UINT32_C(66962) },
    { UINT32_C(66964), UINT32_C(66965) },
    { UINT32_C(66967), UINT32_C(66977) },
    { UINT32_C(66979), UINT32_C(66993) },
    { UINT32_C(66995), UINT32_C(67001) },
    { UINT32_C(67003), UINT32_C(67004) },
    { UINT32_C(67008), UINT32_C(67059) },
    { UINT32_C(67072), UINT32_C(67382) },
    { UINT32_C(67392), UINT32_C(67413) },
    { UINT32_C(67424), UINT32_C(67431) },
    { UINT32_C(67456), UINT32_C(67461) },
    { UINT32_C(67463), UINT32_C(67504) },
    { UINT32_C(67506), UINT32_C(67514) },
    { UINT32_C(67584), UINT32_C(67589) },
    { UINT32_C(67592), UINT32_C(67592) },
    { UINT32_C(67594), UINT32_C(67637) },
    { UINT32_C(67639), UINT32_C(67640) },
    { UINT32_C(67644), UINT32_C(67644) },
    { UINT32_C(67647), UINT32_C(67669) },
    { UINT32_C(67672), UINT32_C(67702) },
    { UINT32_C(67705), UINT32_C(67742) },
    { UINT32_C(67751), UINT32_C(67759) },
    { UINT32_C(67808), UINT32_C(67826) },
    { UINT32_C(67828), UINT32_C(67829) },
    { UINT32_C(67835), UINT32_C(67867) },
    { UINT32_C(67872), UINT32_C(67897) },
    { UINT32_C(67904), UINT32_C(67929) },
    { UINT32_C(67968), UINT32_C(68023) },
    { UINT32_C(68028), UINT32_C(68047) },
    { UINT32_C(68050), UINT32_C(68099) },
    { UINT32_C(68101), UINT32_C(68102) },
    { UINT32_C(68108), UINT32_C(68115) },
    { UINT32_C(68117), UINT32_C(68119) },
    { UINT32_C(68121), UINT32_C(68149) },
    { UINT32_C(68160), UINT32_C(68168) },
    { UINT32_C(68192), UINT32_C(68222) },
    { UINT32_C(68224), UINT32_C(68255) },
    { UINT32_C(68288), UINT32_C(68295) },
    { UINT32_C(68297), UINT32_C(68324) },
    { UINT32_C(68331), UINT32_C(68335) },
    { UINT32_C(68352), UINT32_C(68405) },
    { UINT32_C(68416), UINT32_C(68437) },
    { UINT32_C(68440), UINT32_C(68466) },
    { UINT32_C(68472), UINT32_C(68497) },
    { UINT32_C(68521), UINT32_C(68527) },
    { UINT32_C(68608), UINT32_C(68680) },
    { UINT32_C(68736), UINT32_C(68786) },
    { UINT32_C(68800), UINT32_C(68850) },
    { UINT32_C(68858), UINT32_C(68903) },
    { UINT32_C(68912), UINT32_C(68921) },
    { UINT32_C(68928), UINT32_C(68965) },
    { UINT32_C(68969), UINT32_C(68969) },
    { UINT32_C(68975), UINT32_C(68997) },
    { UINT32_C(69216), UINT32_C(69246) },
    { UINT32_C(69248), UINT32_C(69289) },
    { UINT32_C(69291), UINT32_C(69292) },
    { UINT32_C(69296), UINT32_C(69297) },
    { UINT32_C(69314), UINT32_C(69319) },
    { UINT32_C(69370), UINT32_C(69372) },
    { UINT32_C(69376), UINT32_C(69415) },
    { UINT32_C(69424), UINT32_C(69445) },
    { UINT32_C(69457), UINT32_C(69460) },
    { UINT32_C(69488), UINT32_C(69505) },
    { UINT32_C(69552), UINT32_C(69579) },
    { UINT32_C(69600), UINT32_C(69622) },
    { UINT32_C(69632), UINT32_C(69701) },
    { UINT32_C(69714), UINT32_C(69743) },
    { UINT32_C(69745), UINT32_C(69749) },
    { UINT32_C(69760), UINT32_C(69816) },
    { UINT32_C(69826), UINT32_C(69826) },
    { UINT32_C(69840), UINT32_C(69864) },
    { UINT32_C(69872), UINT32_C(69881) },
    { UINT32_C(69888), UINT32_C(69938) },
    { UINT32_C(69942), UINT32_C(69951) },
    { UINT32_C(69956), UINT32_C(69959) },
    { UINT32_C(69968), UINT32_C(70002) },
    { UINT32_C(70006), UINT32_C(70006) },
    { UINT32_C(70016), UINT32_C(70079) },
    { UINT32_C(70081), UINT32_C(70084) },
    { UINT32_C(70094), UINT32_C(70106) },
    { UINT32_C(70108), UINT32_C(70108) },
    { UINT32_C(70113), UINT32_C(70132) },
    { UINT32_C(70144), UINT32_C(70161) },
    { UINT32_C(70163), UINT32_C(70196) },
    { UINT32_C(70199), UINT32_C(70199) },
    { UINT32_C(70206), UINT32_C(70209) },
    { UINT32_C(70272), UINT32_C(70278) },
    { UINT32_C(70280), UINT32_C(70280) },
    { UINT32_C(70282), UINT32_C(70285) },
    { UINT32_C(70287), UINT32_C(70301) },
    { UINT32_C(70303), UINT32_C(70312) },
    { UINT32_C(70320), UINT32_C(70376) },
    { UINT32_C(70384), UINT32_C(70393) },
    { UINT32_C(70400), UINT32_C(70403) },
    { UINT32_C(70405), UINT32_C(70412) },
    { UINT32_C(70415), UINT32_C(70416) },
    { UINT32_C(70419), UINT32_C(70440) },
    { UINT32_C(70442), UINT32_C(70448) },
    { UINT32_C(70450), UINT32_C(70451) },
    { UINT32_C(70453), UINT32_C(70457) },
    { UINT32_C(70461), UINT32_C(70468) },
    { UINT32_C(70471), UINT32_C(70472) },
    { UINT32_C(70475), UINT32_C(70476) },
    { UINT32_C(70480), UINT32_C(70480) },
    { UINT32_C(70487), UINT32_C(70487) },
    { UINT32_C(70493), UINT32_C(70499) },
    { UINT32_C(70528), UINT32_C(70537) },
    { UINT32_C(70539), UINT32_C(70539) },
    { UINT32_C(70542), UINT32_C(70542) },
    { UINT32_C(70544), UINT32_C(70581) },
    { UINT32_C(70583), UINT32_C(70592) },
    { UINT32_C(70594), UINT32_C(70594) },
    { UINT32_C(70597), UINT32_C(70597) },
    { UINT32_C(70599), UINT32_C(70602) },
    { UINT32_C(70604), UINT32_C(70605) },
    { UINT32_C(70609), UINT32_C(70609) },
    { UINT32_C(70611), UINT32_C(70611) },
    { UINT32_C(70656), UINT32_C(70721) },
    { UINT32_C(70723), UINT32_C(70725) },
    { UINT32_C(70727), UINT32_C(70730) },
    { UINT32_C(70736), UINT32_C(70745) },
    { UINT32_C(70751), UINT32_C(70753) },
    { UINT32_C(70784), UINT32_C(70849) },
    { UINT32_C(70852), UINT32_C(70853) },
    { UINT32_C(70855), UINT32_C(70855) },
    { UINT32_C(70864), UINT32_C(70873) },
    { UINT32_C(71040), UINT32_C(71093) },
    { UINT32_C(71096), UINT32_C(71102) },
    { UINT32_C(71128), UINT32_C(71133) },
    { UINT32_C(71168), UINT32_C(71230) },
    { UINT32_C(71232), UINT32_C(71232) },
    { UINT32_C(71236), UINT32_C(71236) },
    { UINT32_C(71248), UINT32_C(71257) },
    { UINT32_C(71296), UINT32_C(71349) },
    { UINT32_C(71352), UINT32_C(71352) },
    { UINT32_C(71360), UINT32_C(71369) },
    { UINT32_C(71376), UINT32_C(71395) },
    { UINT32_C(71424), UINT32_C(71450) },
    { UINT32_C(71453), UINT32_C(71466) },
    { UINT32_C(71472), UINT32_C(71483) },
    { UINT32_C(71488), UINT32_C(71494) },
    { UINT32_C(71680), UINT32_C(71736) },
    { UINT32_C(71840), UINT32_C(71922) },
    { UINT32_C(71935), UINT32_C(71942) },
    { UINT32_C(71945), UINT32_C(71945) },
    { UINT32_C(71948), UINT32_C(71955) },
    { UINT32_C(71957), UINT32_C(71958) },
    { UINT32_C(71960), UINT32_C(71989) },
    { UINT32_C(71991), UINT32_C(71992) },
    { UINT32_C(71995), UINT32_C(71996) },
    { UINT32_C(71999), UINT32_C(72002) },
    { UINT32_C(72016), UINT32_C(72025) },
    { UINT32_C(72096), UINT32_C(72103) },
    { UINT32_C(72106), UINT32_C(72151) },
    { UINT32_C(72154), UINT32_C(72159) },
    { UINT32_C(72161), UINT32_C(72161) },
    { UINT32_C(72163), UINT32_C(72164) },
    { UINT32_C(72192), UINT32_C(72242) },
    { UINT32_C(72245), UINT32_C(72254) },
    { UINT32_C(72272), UINT32_C(72343) },
    { UINT32_C(72349), UINT32_C(72349) },
    { UINT32_C(72368), UINT32_C(72440) },
    { UINT32_C(72544), UINT32_C(72551) },
    { UINT32_C(72640), UINT32_C(72672) },
    { UINT32_C(72688), UINT32_C(72697) },
    { UINT32_C(72704), UINT32_C(72712) },
    { UINT32_C(72714), UINT32_C(72758) },
    { UINT32_C(72760), UINT32_C(72766) },
    { UINT32_C(72768), UINT32_C(72768) },
    { UINT32_C(72784), UINT32_C(72812) },
    { UINT32_C(72818), UINT32_C(72847) },
    { UINT32_C(72850), UINT32_C(72871) },
    { UINT32_C(72873), UINT32_C(72886) },
    { UINT32_C(72960), UINT32_C(72966) },
    { UINT32_C(72968), UINT32_C(72969) },
    { UINT32_C(72971), UINT32_C(73014) },
    { UINT32_C(73018), UINT32_C(73018) },
    { UINT32_C(73020), UINT32_C(73021) },
    { UINT32_C(73023), UINT32_C(73025) },
    { UINT32_C(73027), UINT32_C(73027) },
    { UINT32_C(73030), UINT32_C(73031) },
    { UINT32_C(73040), UINT32_C(73049) },
    { UINT32_C(73056), UINT32_C(73061) },
    { UINT32_C(73063), UINT32_C(73064) },
    { UINT32_C(73066), UINT32_C(73102) },
    { UINT32_C(73104), UINT32_C(73105) },
    { UINT32_C(73107), UINT32_C(73110) },
    { UINT32_C(73112), UINT32_C(73112) },
    { UINT32_C(73120), UINT32_C(73129) },
    { UINT32_C(73136), UINT32_C(73179) },
    { UINT32_C(73184), UINT32_C(73193) },
    { UINT32_C(73440), UINT32_C(73462) },
    { UINT32_C(73472), UINT32_C(73488) },
    { UINT32_C(73490), UINT32_C(73530) },
    { UINT32_C(73534), UINT32_C(73536) },
    { UINT32_C(73552), UINT32_C(73561) },
    { UINT32_C(73648), UINT32_C(73648) },
    { UINT32_C(73664), UINT32_C(73684) },
    { UINT32_C(73728), UINT32_C(74649) },
    { UINT32_C(74752), UINT32_C(74862) },
    { UINT32_C(74880), UINT32_C(75075) },
    { UINT32_C(77712), UINT32_C(77808) },
    { UINT32_C(77824), UINT32_C(78895) },
    { UINT32_C(78913), UINT32_C(78918) },
    { UINT32_C(78944), UINT32_C(82938) },
    { UINT32_C(82944), UINT32_C(83526) },
    { UINT32_C(90368), UINT32_C(90414) },
    { UINT32_C(90416), UINT32_C(90425) },
    { UINT32_C(92160), UINT32_C(92728) },
    { UINT32_C(92736), UINT32_C(92766) },
    { UINT32_C(92768), UINT32_C(92777) },
    { UINT32_C(92784), UINT32_C(92862) },
    { UINT32_C(92864), UINT32_C(92873) },
    { UINT32_C(92880), UINT32_C(92909) },
    { UINT32_C(92928), UINT32_C(92975) },
    { UINT32_C(92992), UINT32_C(92995) },
    { UINT32_C(93008), UINT32_C(93017) },
    { UINT32_C(93019), UINT32_C(93025) },
    { UINT32_C(93027), UINT32_C(93047) },
    { UINT32_C(93053), UINT32_C(93071) },
    { UINT32_C(93504), UINT32_C(93548) },
    { UINT32_C(93552), UINT32_C(93561) },
    { UINT32_C(93760), UINT32_C(93846) },
    { UINT32_C(93856), UINT32_C(93880) },
    { UINT32_C(93883), UINT32_C(93907) },
    { UINT32_C(93952), UINT32_C(94026) },
    { UINT32_C(94031), UINT32_C(94087) },
    { UINT32_C(94095), UINT32_C(94111) },
    { UINT32_C(94176), UINT32_C(94177) },
    { UINT32_C(94179), UINT32_C(94179) },
    { UINT32_C(94192), UINT32_C(94198) },
    { UINT32_C(94208), UINT32_C(101589) },
    { UINT32_C(101631), UINT32_C(101662) },
    { UINT32_C(101760), UINT32_C(101874) },
    { UINT32_C(110576), UINT32_C(110579) },
    { UINT32_C(110581), UINT32_C(110587) },
    { UINT32_C(110589), UINT32_C(110590) },
    { UINT32_C(110592), UINT32_C(110882) },
    { UINT32_C(110898), UINT32_C(110898) },
    { UINT32_C(110928), UINT32_C(110930) },
    { UINT32_C(110933), UINT32_C(110933) },
    { UINT32_C(110948), UINT32_C(110951) },
    { UINT32_C(110960), UINT32_C(111355) },
    { UINT32_C(113664), UINT32_C(113770) },
    { UINT32_C(113776), UINT32_C(113788) },
    { UINT32_C(113792), UINT32_C(113800) },
    { UINT32_C(113808), UINT32_C(113817) },
    { UINT32_C(113822), UINT32_C(113822) },
    { UINT32_C(118000), UINT32_C(118009) },
    { UINT32_C(119488), UINT32_C(119507) },
    { UINT32_C(119520), UINT32_C(119539) },
    { UINT32_C(119648), UINT32_C(119672) },
    { UINT32_C(119808), UINT32_C(119892) },
    { UINT32_C(119894), UINT32_C(119964) },
    { UINT32_C(119966), UINT32_C(119967) },
    { UINT32_C(119970), UINT32_C(119970) },
    { UINT32_C(119973), UINT32_C(119974) },
    { UINT32_C(119977), UINT32_C(119980) },
    { UINT32_C(119982), UINT32_C(119993) },
    { UINT32_C(119995), UINT32_C(119995) },
    { UINT32_C(119997), UINT32_C(120003) },
    { UINT32_C(120005), UINT32_C(120069) },
    { UINT32_C(120071), UINT32_C(120074) },
    { UINT32_C(120077), UINT32_C(120084) },
    { UINT32_C(120086), UINT32_C(120092) },
    { UINT32_C(120094), UINT32_C(120121) },
    { UINT32_C(120123), UINT32_C(120126) },
    { UINT32_C(120128), UINT32_C(120132) },
    { UINT32_C(120134), UINT32_C(120134) },
    { UINT32_C(120138), UINT32_C(120144) },
    { UINT32_C(120146), UINT32_C(120485) },
    { UINT32_C(120488), UINT32_C(120512) },
    { UINT32_C(120514), UINT32_C(120538) },
    { UINT32_C(120540), UINT32_C(120570) },
    { UINT32_C(120572), UINT32_C(120596) },
    { UINT32_C(120598), UINT32_C(120628) },
    { UINT32_C(120630), UINT32_C(120654) },
    { UINT32_C(120656), UINT32_C(120686) },
    { UINT32_C(120688), UINT32_C(120712) },
    { UINT32_C(120714), UINT32_C(120744) },
    { UINT32_C(120746), UINT32_C(120770) },
    { UINT32_C(120772), UINT32_C(120779) },
    { UINT32_C(120782), UINT32_C(120831) },
    { UINT32_C(122624), UINT32_C(122654) },
    { UINT32_C(122661), UINT32_C(122666) },
    { UINT32_C(122880), UINT32_C(122886) },
    { UINT32_C(122888), UINT32_C(122904) },
    { UINT32_C(122907), UINT32_C(122913) },
    { UINT32_C(122915), UINT32_C(122916) },
    { UINT32_C(122918), UINT32_C(122922) },
    { UINT32_C(122928), UINT32_C(122989) },
    { UINT32_C(123023), UINT32_C(123023) },
    { UINT32_C(123136), UINT32_C(123180) },
    { UINT32_C(123191), UINT32_C(123197) },
    { UINT32_C(123200), UINT32_C(123209) },
    { UINT32_C(123214), UINT32_C(123214) },
    { UINT32_C(123536), UINT32_C(123565) },
    { UINT32_C(123584), UINT32_C(123627) },
    { UINT32_C(123632), UINT32_C(123641) },
    { UINT32_C(124112), UINT32_C(124139) },
    { UINT32_C(124144), UINT32_C(124153) },
    { UINT32_C(124368), UINT32_C(124397) },
    { UINT32_C(124400), UINT32_C(124410) },
    { UINT32_C(124608), UINT32_C(124638) },
    { UINT32_C(124640), UINT32_C(124661) },
    { UINT32_C(124670), UINT32_C(124671) },
    { UINT32_C(124896), UINT32_C(124902) },
    { UINT32_C(124904), UINT32_C(124907) },
    { UINT32_C(124909), UINT32_C(124910) },
    { UINT32_C(124912), UINT32_C(124926) },
    { UINT32_C(124928), UINT32_C(125124) },
    { UINT32_C(125127), UINT32_C(125135) },
    { UINT32_C(125184), UINT32_C(125251) },
    { UINT32_C(125255), UINT32_C(125255) },
    { UINT32_C(125259), UINT32_C(125259) },
    { UINT32_C(125264), UINT32_C(125273) },
    { UINT32_C(126065), UINT32_C(126123) },
    { UINT32_C(126125), UINT32_C(126127) },
    { UINT32_C(126129), UINT32_C(126132) },
    { UINT32_C(126209), UINT32_C(126253) },
    { UINT32_C(126255), UINT32_C(126269) },
    { UINT32_C(126464), UINT32_C(126467) },
    { UINT32_C(126469), UINT32_C(126495) },
    { UINT32_C(126497), UINT32_C(126498) },
    { UINT32_C(126500), UINT32_C(126500) },
    { UINT32_C(126503), UINT32_C(126503) },
    { UINT32_C(126505), UINT32_C(126514) },
    { UINT32_C(126516), UINT32_C(126519) },
    { UINT32_C(126521), UINT32_C(126521) },
    { UINT32_C(126523), UINT32_C(126523) },
    { UINT32_C(126530), UINT32_C(126530) },
    { UINT32_C(126535), UINT32_C(126535) },
    { UINT32_C(126537), UINT32_C(126537) },
    { UINT32_C(126539), UINT32_C(126539) },
    { UINT32_C(126541), UINT32_C(126543) },
    { UINT32_C(126545), UINT32_C(126546) },
    { UINT32_C(126548), UINT32_C(126548) },
    { UINT32_C(126551), UINT32_C(126551) },
    { UINT32_C(126553), UINT32_C(126553) },
    { UINT32_C(126555), UINT32_C(126555) },
    { UINT32_C(126557), UINT32_C(126557) },
    { UINT32_C(126559), UINT32_C(126559) },
    { UINT32_C(126561), UINT32_C(126562) },
    { UINT32_C(126564), UINT32_C(126564) },
    { UINT32_C(126567), UINT32_C(126570) },
    { UINT32_C(126572), UINT32_C(126578) },
    { UINT32_C(126580), UINT32_C(126583) },
    { UINT32_C(126585), UINT32_C(126588) },
    { UINT32_C(126590), UINT32_C(126590) },
    { UINT32_C(126592), UINT32_C(126601) },
    { UINT32_C(126603), UINT32_C(126619) },
    { UINT32_C(126625), UINT32_C(126627) },
    { UINT32_C(126629), UINT32_C(126633) },
    { UINT32_C(126635), UINT32_C(126651) },
    { UINT32_C(127232), UINT32_C(127244) },
    { UINT32_C(127280), UINT32_C(127305) },
    { UINT32_C(127312), UINT32_C(127337) },
    { UINT32_C(127344), UINT32_C(127369) },
    { UINT32_C(130032), UINT32_C(130041) },
    { UINT32_C(131072), UINT32_C(173791) },
    { UINT32_C(173824), UINT32_C(178205) },
    { UINT32_C(178208), UINT32_C(183981) },
    { UINT32_C(183984), UINT32_C(191456) },
    { UINT32_C(191472), UINT32_C(192093) },
    { UINT32_C(194560), UINT32_C(195101) },
    { UINT32_C(196608), UINT32_C(201546) },
    { UINT32_C(201552), UINT32_C(210041) },
};
static int mimi_unicode_in_ranges(uint32_t value, const MimiUnicodeRange *ranges, size_t count) {
    size_t low = 0, high = count;
    while (low < high) {
        size_t middle = low + (high - low) / 2;
        if (value < ranges[middle].first) high = middle;
        else if (value > ranges[middle].last) low = middle + 1;
        else return 1;
    }
    return 0;
}
int mimi_unicode_alphabetic(uint64_t value) {
    return value <= UINT32_MAX && mimi_unicode_in_ranges((uint32_t)value, MIMI_UNICODE_ALPHABETIC, sizeof(MIMI_UNICODE_ALPHABETIC) / sizeof(MIMI_UNICODE_ALPHABETIC[0]));
}
int mimi_unicode_alphanumeric(uint64_t value) {
    return value <= UINT32_MAX && mimi_unicode_in_ranges((uint32_t)value, MIMI_UNICODE_ALPHANUMERIC, sizeof(MIMI_UNICODE_ALPHANUMERIC) / sizeof(MIMI_UNICODE_ALPHANUMERIC[0]));
}
