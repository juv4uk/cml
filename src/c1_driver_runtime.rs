//! C1-only generated-C runtime support for selfhost driver (#630).
//!
//! Representation mechanics only: bounded stdin, canonical Value rendering,
//! SHA-256, and output. No DomainIdentity meaning/role/proof selection lives here.

pub(crate) const C1_DRIVER_RUNTIME: &str = r##"
#ifndef CML_C1_INPUT_MAX
#define CML_C1_INPUT_MAX (16u * 1024u * 1024u)
#endif
#ifndef CML_C1_CANONICAL_MAX
#define CML_C1_CANONICAL_MAX (4u * 1024u * 1024u)
#endif

typedef struct {
    uint8_t data[64];
    uint32_t datalen;
    uint64_t bitlen;
    uint32_t state[8];
} C1Sha256Ctx;

static const uint32_t c1_sha256_k[64] = {
    0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
    0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
    0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
    0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
    0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
    0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
    0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
    0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u
};

static uint32_t c1_rotr32(uint32_t value, uint32_t count) {
    return (value >> count) | (value << (32u - count));
}

static void c1_sha256_transform(C1Sha256Ctx *ctx, const uint8_t data[64]) {
    uint32_t m[64];
    for (uint32_t i = 0; i < 16; ++i) {
        uint32_t j = i * 4u;
        m[i] = ((uint32_t)data[j] << 24) | ((uint32_t)data[j + 1] << 16)
             | ((uint32_t)data[j + 2] << 8) | (uint32_t)data[j + 3];
    }
    for (uint32_t i = 16; i < 64; ++i) {
        uint32_t s0 = c1_rotr32(m[i - 15], 7) ^ c1_rotr32(m[i - 15], 18) ^ (m[i - 15] >> 3);
        uint32_t s1 = c1_rotr32(m[i - 2], 17) ^ c1_rotr32(m[i - 2], 19) ^ (m[i - 2] >> 10);
        m[i] = m[i - 16] + s0 + m[i - 7] + s1;
    }

    uint32_t a=ctx->state[0], b=ctx->state[1], c=ctx->state[2], d=ctx->state[3];
    uint32_t e=ctx->state[4], f=ctx->state[5], g=ctx->state[6], h=ctx->state[7];
    for (uint32_t i = 0; i < 64; ++i) {
        uint32_t s1 = c1_rotr32(e,6) ^ c1_rotr32(e,11) ^ c1_rotr32(e,25);
        uint32_t ch = (e & f) ^ ((~e) & g);
        uint32_t t1 = h + s1 + ch + c1_sha256_k[i] + m[i];
        uint32_t s0 = c1_rotr32(a,2) ^ c1_rotr32(a,13) ^ c1_rotr32(a,22);
        uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t t2 = s0 + maj;
        h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;
    }
    ctx->state[0]+=a; ctx->state[1]+=b; ctx->state[2]+=c; ctx->state[3]+=d;
    ctx->state[4]+=e; ctx->state[5]+=f; ctx->state[6]+=g; ctx->state[7]+=h;
}

static void c1_sha256_init(C1Sha256Ctx *ctx) {
    ctx->datalen = 0; ctx->bitlen = 0;
    ctx->state[0]=0x6a09e667u; ctx->state[1]=0xbb67ae85u;
    ctx->state[2]=0x3c6ef372u; ctx->state[3]=0xa54ff53au;
    ctx->state[4]=0x510e527fu; ctx->state[5]=0x9b05688cu;
    ctx->state[6]=0x1f83d9abu; ctx->state[7]=0x5be0cd19u;
}

static void c1_sha256_update(C1Sha256Ctx *ctx, const uint8_t *data, size_t len) {
    for (size_t i=0; i<len; ++i) {
        ctx->data[ctx->datalen++] = data[i];
        if (ctx->datalen == 64) {
            c1_sha256_transform(ctx, ctx->data);
            ctx->bitlen += 512;
            ctx->datalen = 0;
        }
    }
}

static void c1_sha256_final(C1Sha256Ctx *ctx, uint8_t hash[32]) {
    uint32_t i = ctx->datalen;
    ctx->data[i++] = 0x80u;
    if (i > 56) {
        while (i < 64) ctx->data[i++] = 0;
        c1_sha256_transform(ctx, ctx->data);
        i = 0;
    }
    while (i < 56) ctx->data[i++] = 0;
    ctx->bitlen += (uint64_t)ctx->datalen * 8u;
    for (uint32_t n=0; n<8; ++n)
        ctx->data[63u-n] = (uint8_t)(ctx->bitlen >> (n*8u));
    c1_sha256_transform(ctx, ctx->data);

    for (i=0; i<4; ++i) {
        for (uint32_t word=0; word<8; ++word)
            hash[word*4u+i] = (uint8_t)(ctx->state[word] >> (24u-i*8u));
    }
}

static char *c1_sha256_hex_bytes(const uint8_t *bytes, size_t len) {
    static const char hex[] = "0123456789abcdef";
    uint8_t digest[32];
    C1Sha256Ctx ctx;
    c1_sha256_init(&ctx);
    c1_sha256_update(&ctx, bytes, len);
    c1_sha256_final(&ctx, digest);
    char *out = checked_malloc(65);
    for (size_t i=0; i<32; ++i) {
        out[i*2] = hex[digest[i] >> 4];
        out[i*2+1] = hex[digest[i] & 0x0fu];
    }
    out[64] = '\0';
    return out;
}

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} C1CanonicalBuffer;

static void c1_canon_init(C1CanonicalBuffer *out) {
    out->cap = (size_t)CML_C1_CANONICAL_MAX;
    out->data = checked_malloc(out->cap + 1);
    out->len = 0;
    out->data[0] = '\0';
}
static void c1_canon_putc(C1CanonicalBuffer *out, char ch) {
    if (out->len >= out->cap) runtime_error("Canonical", "C1 canonical value exceeds limit");
    out->data[out->len++] = ch;
    out->data[out->len] = '\0';
}
static void c1_canon_puts(C1CanonicalBuffer *out, const char *text) {
    size_t len = strlen(text);
    if (len > out->cap - out->len) runtime_error("Canonical", "C1 canonical value exceeds limit");
    memcpy(out->data + out->len, text, len);
    out->len += len;
    out->data[out->len] = '\0';
}
static void c1_canon_unsigned_binary(C1CanonicalBuffer *out, unsigned long value) {
    char bits[sizeof(unsigned long) * 8u + 1u];
    size_t len = 0;
    if (value == 0) { c1_canon_putc(out, '0'); return; }
    while (value != 0) { bits[len++] = (value & 1ul) ? '1' : '0'; value >>= 1; }
    while (len != 0) c1_canon_putc(out, bits[--len]);
}
static void c1_canon_signed_binary(C1CanonicalBuffer *out, long value) {
    if (value < 0) {
        c1_canon_putc(out, '-');
        unsigned long magnitude = (unsigned long)(-(value + 1));
        magnitude += 1ul;
        c1_canon_unsigned_binary(out, magnitude);
    } else {
        c1_canon_unsigned_binary(out, (unsigned long)value);
    }
}

static void c1_canonical_emit(C1CanonicalBuffer *out, Value *value);

static void c1_canonical_pair(C1CanonicalBuffer *out, Value *value) {
    c1_canon_putc(out, '(');
    int first = 1;
    Value *cursor = value;
    while (cursor->tag == TAG_CONS) {
        if (!first) c1_canon_putc(out, ' ');
        c1_canonical_emit(out, cursor->u.cons.car);
        cursor = cursor->u.cons.cdr;
        first = 0;
    }
    if (cursor->tag != TAG_NIL) {
        c1_canon_puts(out, " . ");
        c1_canonical_emit(out, cursor);
    }
    c1_canon_putc(out, ')');
}

static void c1_canonical_string(C1CanonicalBuffer *out, const char *text) {
    c1_canon_putc(out, '"');
    for (const unsigned char *p=(const unsigned char *)text; *p; ++p) {
        switch (*p) {
            case '"': c1_canon_puts(out, "\\\""); break;
            case '\\': c1_canon_puts(out, "\\\\"); break;
            case '\n': c1_canon_puts(out, "\\n"); break;
            case '\t': c1_canon_puts(out, "\\t"); break;
            default: c1_canon_putc(out, (char)*p); break;
        }
    }
    c1_canon_putc(out, '"');
}

static void c1_canonical_emit(C1CanonicalBuffer *out, Value *value) {
    switch (value->tag) {
        case TAG_NIL: c1_canon_puts(out, "()"); break;
        case TAG_INT:
            c1_canon_puts(out, "#q2:");
            c1_canon_signed_binary(out, value->u.i);
            c1_canon_puts(out, "/1");
            break;
        case TAG_RATIONAL:
            c1_canon_puts(out, "#q2:");
            c1_canon_signed_binary(out, value->u.rat.num);
            c1_canon_putc(out, '/');
            c1_canon_unsigned_binary(out, (unsigned long)value->u.rat.den);
            break;
        case TAG_SYM: c1_canon_puts(out, value->u.sym); break;
        case TAG_STRING: c1_canonical_string(out, value->u.str); break;
        case TAG_PREDICATE_BIT: c1_canon_putc(out, value->u.predicate_bit ? '1' : '0'); break;
        case TAG_DOMAIN_IDENTITY:
            for (int bit=(int)value->u.domain_identity.width-1; bit>=0; --bit)
                c1_canon_putc(out, (value->u.domain_identity.packed_bits & (1u << bit)) ? '1' : '0');
            break;
        case TAG_CONS: c1_canonical_pair(out, value); break;
        default: runtime_error("Canonical", "unsupported C1 value in canonical artifact"); break;
    }
}

static char *c1_canonical_value_string(Value *value) {
    C1CanonicalBuffer out;
    c1_canon_init(&out);
    c1_canonical_emit(&out, value);
    return out.data;
}

typedef struct {
    uint8_t *data;
    size_t len;
    size_t cap;
} C1EvidenceBuffer;

static void c1_evidence_init(C1EvidenceBuffer *out) {
    out->cap = (size_t)CML_C1_CANONICAL_MAX;
    out->data = checked_malloc(out->cap);
    out->len = 0;
}

static void c1_evidence_put_byte(C1EvidenceBuffer *out, uint8_t byte) {
    if (out->len >= out->cap)
        runtime_error("Canonical", "C1 compiler evidence exceeds limit");
    out->data[out->len++] = byte;
}

static void c1_evidence_put_bytes(
    C1EvidenceBuffer *out,
    const uint8_t *bytes,
    size_t len
) {
    if (len > out->cap - out->len)
        runtime_error("Canonical", "C1 compiler evidence exceeds limit");
    memcpy(out->data + out->len, bytes, len);
    out->len += len;
}

static void c1_evidence_put_u64_le(C1EvidenceBuffer *out, uint64_t value) {
    for (uint32_t shift = 0; shift < 64; shift += 8)
        c1_evidence_put_byte(out, (uint8_t)(value >> shift));
}

static void c1_encode_compiler_evidence(C1EvidenceBuffer *out, Value *value) {
    switch (value->tag) {
        case TAG_NIL:
            c1_evidence_put_byte(out, 0x00u);
            break;
        case TAG_DOMAIN_IDENTITY:
            c1_evidence_put_byte(out, 0x01u);
            c1_evidence_put_byte(out, value->u.domain_identity.width);
            c1_evidence_put_byte(out, value->u.domain_identity.packed_bits);
            break;
        case TAG_SYM: {
            size_t len = strlen(value->u.sym);
            c1_evidence_put_byte(out, 0x02u);
            c1_evidence_put_u64_le(out, (uint64_t)len);
            c1_evidence_put_bytes(out, (const uint8_t *)value->u.sym, len);
            break;
        }
        case TAG_STRING: {
            size_t len = strlen(value->u.str);
            c1_evidence_put_byte(out, 0x03u);
            c1_evidence_put_u64_le(out, (uint64_t)len);
            c1_evidence_put_bytes(out, (const uint8_t *)value->u.str, len);
            break;
        }
        case TAG_CONS:
            c1_evidence_put_byte(out, 0x04u);
            c1_encode_compiler_evidence(out, value->u.cons.car);
            c1_encode_compiler_evidence(out, value->u.cons.cdr);
            break;
        default:
            runtime_error(
                "Canonical",
                "canonical compiler-value hash rejects this runtime value"
            );
            break;
    }
}

static Value *builtin_canonical_value_sha256(Value *args, Value *env) {
    (void)env;
    require_arity(args, 1, "canonical-value-sha256");
    C1EvidenceBuffer encoded;
    c1_evidence_init(&encoded);
    c1_encode_compiler_evidence(&encoded, arg_at(args, 0));
    char *digest = c1_sha256_hex_bytes(encoded.data, encoded.len);
    free(encoded.data);
    return mk_string(digest);
}

static void c1_write_compiler_evidence(Value *value) {
    C1EvidenceBuffer encoded;
    c1_evidence_init(&encoded);
    c1_encode_compiler_evidence(&encoded, value);
    if (fwrite(encoded.data, 1, encoded.len, stdout) != encoded.len)
        runtime_error("IO", "failed writing canonical compiler evidence");
    free(encoded.data);
}

static uint8_t *c1_read_stdin_all(size_t *out_len) {
    size_t cap = (size_t)CML_C1_INPUT_MAX + 1u;
    uint8_t *bytes = checked_malloc(cap);
    size_t len = fread(bytes, 1, cap, stdin);
    if (ferror(stdin)) runtime_error("IO", "failed reading C1 stdin");
    if (len > (size_t)CML_C1_INPUT_MAX)
        runtime_error("Wire", "C1 input exceeds bounded transport limit");
    if (!feof(stdin)) {
        int extra = fgetc(stdin);
        if (extra != EOF) runtime_error("Wire", "C1 input exceeds bounded transport limit");
    }
    *out_len = len;
    return bytes;
}

static void c1_print_canonical(Value *value) {
    char *canonical = c1_canonical_value_string(value);
    fputs(canonical, stdout);
    fputc('\n', stdout);
    free(canonical);
}
"##;
