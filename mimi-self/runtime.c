#include <stdint.h>
#include <errno.h>
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
