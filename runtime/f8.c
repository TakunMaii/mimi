#include <math.h>
#include <stdint.h>
#include <string.h>

static double decode_magnitude(uint8_t bits) {
    unsigned exponent = (bits >> 3) & 15u;
    unsigned fraction = bits & 7u;
    if (exponent == 0) {
        return (double)fraction / 512.0;
    }
    unsigned significand = 8u + fraction;
    if (exponent <= 10) {
        return (double)significand / (double)(1u << (10u - exponent));
    }
    return (double)(significand * (1u << (exponent - 10u)));
}

static double decode(uint64_t value) {
    uint8_t bits = (uint8_t)value;
    uint8_t magnitude = bits & 127u;
    if (magnitude == 127u) {
        return NAN;
    }
    double result = decode_magnitude(magnitude);
    return (bits & 128u) ? -result : result;
}

static uint64_t encode(double value) {
    if (value != value) {
        return 127u;
    }
    uint64_t raw;
    memcpy(&raw, &value, sizeof(raw));
    uint8_t sign = (uint8_t)((raw >> 63) << 7);
    double magnitude = sign ? -value : value;
    if (magnitude >= 448.0) {
        return sign | 126u;
    }
    uint8_t best = 0;
    double best_distance = magnitude;
    for (uint8_t candidate = 1; candidate <= 126u; candidate++) {
        double decoded = decode_magnitude(candidate);
        double distance = magnitude > decoded ? magnitude - decoded : decoded - magnitude;
        if (distance < best_distance ||
            (distance == best_distance && (candidate & 1u) == 0u)) {
            best = candidate;
            best_distance = distance;
        }
    }
    return sign | best;
}

uint64_t mimi_f8_add(uint64_t left, uint64_t right) {
    return encode(decode(left) + decode(right));
}

uint64_t mimi_f8_sub(uint64_t left, uint64_t right) {
    return encode(decode(left) - decode(right));
}

uint64_t mimi_f8_mul(uint64_t left, uint64_t right) {
    return encode(decode(left) * decode(right));
}

uint64_t mimi_f8_div(uint64_t left, uint64_t right) {
    return encode(decode(left) / decode(right));
}

uint64_t mimi_f8_neg(uint64_t value) {
    return (uint8_t)value ^ 128u;
}

uint64_t mimi_f8_eq(uint64_t left, uint64_t right) {
    return decode(left) == decode(right);
}

uint64_t mimi_f8_ne(uint64_t left, uint64_t right) {
    return decode(left) != decode(right);
}

uint64_t mimi_f8_lt(uint64_t left, uint64_t right) {
    return decode(left) < decode(right);
}

uint64_t mimi_f8_le(uint64_t left, uint64_t right) {
    return decode(left) <= decode(right);
}

uint64_t mimi_f8_gt(uint64_t left, uint64_t right) {
    return decode(left) > decode(right);
}

uint64_t mimi_f8_ge(uint64_t left, uint64_t right) {
    return decode(left) >= decode(right);
}
