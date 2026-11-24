// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K375: host-side fake SSH peer tracer for the guest sshd.
//
// This is test tooling only. It is built with the host cc, is never linked
// into the product and is never staged in the image. It implements the
// version exchange (RFC 4253 section 4.2) and the binary packet format
// (RFC 4251 section 6) from the specifications, with no donor code.
//
// The peer connects, timestamps every byte with CLOCK_MONOTONIC, optionally
// sends `SSH-2.0-FakePeer_1.0\r\n`, a syntactically valid unencrypted KEXINIT
// (SSH_MSG_KEXINIT, 20) offering curve25519-sha256/ssh-ed25519, and a
// KEX_ECDH_INIT (30) with the X25519 base point (u=9) so the guest answers
// with KEX_ECDH_REPLY (31). Records are parsed and annotated with their
// packet_length, padding_length, payload length and message type, so a
// zero-byte stall, a truncated record or a mis-framed record is visible.
//
// Exit codes: 0 connected and the session ran to EOF/idle; 2 setup or usage
// error (never a pass/fail judgement of the peer under test).

#define _POSIX_C_SOURCE 200809L

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#define MAX_READ 65536
#define DUMP_BYTES 512
#define CONNECT_TIMEOUT_MS 5000
#define RECORD_LIMIT 262144

struct options {
    const char *host;
    int port;
    int send_banner;
    int send_kexinit;
    int send_ecdh_init;
    int idle_ms;
    const char *log_path;
};

static FILE *log_file;
static struct timespec start_time;

static long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)(ts.tv_sec - start_time.tv_sec) * 1000 +
           (ts.tv_nsec - start_time.tv_nsec) / 1000000;
}

static void emit(const char *fmt, ...) {
    char line[8192];
    va_list args;
    va_start(args, fmt);
    int n = vsnprintf(line, sizeof line, fmt, args);
    va_end(args);
    if (n < 0) {
        return;
    }
    if (log_file != NULL) {
        fputs(line, log_file);
        fputc('\n', log_file);
        fflush(log_file);
    }
    fputs(line, stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

static const char *message_name(unsigned type) {
    switch (type) {
    case 1:
        return "SSH_MSG_DISCONNECT";
    case 2:
        return "SSH_MSG_IGNORE";
    case 3:
        return "SSH_MSG_UNIMPLEMENTED";
    case 4:
        return "SSH_MSG_DEBUG";
    case 5:
        return "SSH_MSG_SERVICE_REQUEST";
    case 6:
        return "SSH_MSG_SERVICE_ACCEPT";
    case 20:
        return "SSH_MSG_KEXINIT";
    case 21:
        return "SSH_MSG_NEWKEYS";
    case 30:
        return "SSH_MSG_KEX_ECDH_INIT";
    case 31:
        return "SSH_MSG_KEX_ECDH_REPLY";
    case 50:
        return "SSH_MSG_USERAUTH_REQUEST";
    case 51:
        return "SSH_MSG_USERAUTH_FAILURE";
    case 52:
        return "SSH_MSG_USERAUTH_SUCCESS";
    case 80:
        return "SSH_MSG_GLOBAL_REQUEST";
    case 90:
        return "SSH_MSG_CHANNEL_OPEN";
    case 91:
        return "SSH_MSG_CHANNEL_OPEN_CONFIRMATION";
    case 92:
        return "SSH_MSG_CHANNEL_OPEN_FAILURE";
    default:
        return "unknown";
    }
}

static void hex_and_printable(const unsigned char *bytes, size_t length, char *hex,
                              size_t hex_size, char *text, size_t text_size) {
    size_t shown = length < DUMP_BYTES ? length : DUMP_BYTES;
    size_t at = 0;
    size_t ta = 0;
    for (size_t index = 0; index < shown; index++) {
        int n = snprintf(hex + at, hex_size - at, index == 0 ? "%02x" : " %02x",
                         bytes[index]);
        if (n < 0 || (size_t)n >= hex_size - at) {
            break;
        }
        at += (size_t)n;
        unsigned char c = bytes[index];
        if (ta + 1 < text_size) {
            text[ta++] = (c >= 0x20 && c < 0x7f) ? (char)c : '.';
        }
    }
    if (length > shown && ta + 1 < text_size) {
        text[ta++] = '+';
    }
    if (hex_size > 0) {
        hex[at] = '\0';
    }
    if (text_size > 0) {
        text[ta] = '\0';
    }
}

static int write_all(int fd, const unsigned char *bytes, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t n = write(fd, bytes + done, length - done);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)n;
    }
    return 0;
}

static size_t build_record(unsigned char *out, size_t capacity, const unsigned char *payload,
                           size_t payload_length) {
    size_t padding = 8 - ((4 + 1 + payload_length) % 8);
    if (padding < 4) {
        padding += 8;
    }
    size_t total = 4 + 1 + payload_length + padding;
    if (total > capacity) {
        return 0;
    }
    uint32_t packet_length = (uint32_t)(1 + payload_length + padding);
    out[0] = (unsigned char)(packet_length >> 24);
    out[1] = (unsigned char)(packet_length >> 16);
    out[2] = (unsigned char)(packet_length >> 8);
    out[3] = (unsigned char)packet_length;
    out[4] = (unsigned char)padding;
    memcpy(out + 5, payload, payload_length);
    unsigned state = (unsigned)now_ms() * 1103515245u + 12345u;
    for (size_t index = 0; index < padding; index++) {
        state = state * 1103515245u + 12345u;
        out[5 + payload_length + index] = (unsigned char)(state >> 16);
    }
    return total;
}

static size_t build_kexinit(unsigned char *out, size_t capacity) {
    static const char *const names[] = {
        "curve25519-sha256",
        "ssh-ed25519",
        "aes128-ctr",
        "aes128-ctr",
        "hmac-sha2-256",
        "hmac-sha2-256",
        "none",
        "none",
        "",
        "",
    };
    unsigned char payload[1024];
    size_t at = 0;
    payload[at++] = 20; /* SSH_MSG_KEXINIT */
    unsigned state = (unsigned)now_ms() * 2654435761u + 1u;
    for (int index = 0; index < 16; index++) {
        state = state * 1103515245u + 12345u;
        payload[at++] = (unsigned char)(state >> 16);
    }
    for (size_t index = 0; index < sizeof names / sizeof names[0]; index++) {
        size_t length = strlen(names[index]);
        payload[at++] = 0;
        payload[at++] = 0;
        payload[at++] = (unsigned char)(length >> 8);
        payload[at++] = (unsigned char)length;
        memcpy(payload + at, names[index], length);
        at += length;
    }
    payload[at++] = 0; /* first_kex_packet_follows */
    payload[at++] = 0; /* reserved */
    payload[at++] = 0;
    payload[at++] = 0;
    payload[at++] = 0;
    return build_record(out, capacity, payload, at);
}

static size_t build_ecdh_init(unsigned char *out, size_t capacity) {
    unsigned char payload[64];
    size_t at = 0;
    payload[at++] = 30; /* SSH_MSG_KEX_ECDH_INIT */
    payload[at++] = 0;
    payload[at++] = 0;
    payload[at++] = 0;
    payload[at++] = 32;
    for (int index = 0; index < 32; index++) {
        payload[at++] = (index == 0) ? 9 : 0; /* X25519 base point u=9 */
    }
    return build_record(out, capacity, payload, at);
}

static void send_bytes(int fd, const char *what, const unsigned char *bytes, size_t length) {
    char hex[3 * DUMP_BYTES + 8];
    char text[DUMP_BYTES + 8];
    hex_and_printable(bytes, length, hex, sizeof hex, text, sizeof text);
    long long before = now_ms();
    if (write_all(fd, bytes, length) != 0) {
        emit("%06lld ms write %s FAILED: %s", before, what, strerror(errno));
        return;
    }
    emit("%06lld ms write %zu bytes (%s): %s |%s|", before, length, what, hex, text);
}

static int connect_timed(const char *host, int port) {
    struct sockaddr_in address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons((uint16_t)port);
    if (inet_pton(AF_INET, host, &address.sin_addr) != 1) {
        emit("error: --host must be an IPv4 address, got `%s`", host);
        return -1;
    }
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) {
        emit("error: socket: %s", strerror(errno));
        return -1;
    }
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0) {
        emit("error: fcntl: %s", strerror(errno));
        close(fd);
        return -1;
    }
    int result = connect(fd, (struct sockaddr *)&address, sizeof address);
    if (result < 0 && errno != EINPROGRESS) {
        emit("error: connect %s:%d: %s", host, port, strerror(errno));
        close(fd);
        return -1;
    }
    if (result < 0) {
        struct pollfd pfd = {.fd = fd, .events = POLLOUT};
        int ready = poll(&pfd, 1, CONNECT_TIMEOUT_MS);
        if (ready <= 0) {
            emit("error: connect %s:%d timed out after %d ms", host, port,
                 CONNECT_TIMEOUT_MS);
            close(fd);
            return -1;
        }
        int error = 0;
        socklen_t length = sizeof error;
        if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &length) < 0 || error != 0) {
            emit("error: connect %s:%d: %s", host, port,
                 error != 0 ? strerror(error) : strerror(errno));
            close(fd);
            return -1;
        }
    }
    if (fcntl(fd, F_SETFL, flags) < 0) {
        emit("error: fcntl restore: %s", strerror(errno));
        close(fd);
        return -1;
    }
    return fd;
}

struct reader {
    unsigned char buffer[MAX_READ];
    size_t length;
    int in_version;
    int saw_version;
    int saw_kexinit;
    size_t total;
    size_t records;
};

static void parse_stream(struct reader *reader) {
    size_t at = 0;
    if (reader->in_version) {
        while (at < reader->length) {
            if (reader->buffer[at] == '\n') {
                at++;
                size_t start = 0;
                while (start < at && (reader->buffer[start] == '\r' ||
                                      reader->buffer[start] == '\n')) {
                    start++;
                }
                size_t end = at;
                while (end > start && (reader->buffer[end - 1] == '\r' ||
                                       reader->buffer[end - 1] == '\n')) {
                    end--;
                }
                char line[256];
                size_t copy = end - start < sizeof line - 1 ? end - start : sizeof line - 1;
                memcpy(line, reader->buffer + start, copy);
                line[copy] = '\0';
                emit("%06lld ms version line: `%s`", now_ms(), line);
                reader->in_version = 0;
                reader->saw_version = 1;
                break;
            }
            at++;
        }
        if (reader->in_version) {
            /* Keep the partial version line for the next read; never drop it. */
            return;
        }
    }
    while (reader->length - at >= 5) {
        uint32_t packet_length = ((uint32_t)reader->buffer[at] << 24) |
                                 ((uint32_t)reader->buffer[at + 1] << 16) |
                                 ((uint32_t)reader->buffer[at + 2] << 8) |
                                 (uint32_t)reader->buffer[at + 3];
        if (packet_length == 0 || packet_length > RECORD_LIMIT) {
            emit("%06lld ms record framing error: packet_length=%u", now_ms(),
                 packet_length);
            at = reader->length;
            break;
        }
        size_t total = 4 + (size_t)packet_length;
        if (reader->length - at < total) {
            break;
        }
        unsigned padding = reader->buffer[at + 4];
        size_t payload = packet_length - 1 - padding;
        unsigned type = payload > 0 ? reader->buffer[at + 5] : 0;
        reader->records++;
        emit("%06lld ms record #%zu total=%zu packet_length=%u padding=%u payload=%zu type=%u (%s)",
             now_ms(), reader->records, total, packet_length, padding, payload, type,
             message_name(type));
        if (type == 20) {
            reader->saw_kexinit = 1;
        }
        at += total;
    }
    if (at > 0) {
        memmove(reader->buffer, reader->buffer + at, reader->length - at);
        reader->length -= at;
    }
}

static void usage(const char *program) {
    fprintf(stderr,
            "usage: %s --host H --port P [--mode client] [--send-banner] "
            "[--send-kexinit] [--send-kex-ecdh-init] [--idle-ms N] [--log FILE]\n",
            program);
}

static int parse_port(const char *text) {
    char *end = NULL;
    long value = strtol(text, &end, 10);
    if (end == NULL || *end != '\0' || value <= 0 || value > 65535) {
        return -1;
    }
    return (int)value;
}

int main(int argc, char **argv) {
    struct options options = {
        .host = "127.0.0.1",
        .port = 22,
        .send_banner = 0,
        .send_kexinit = 0,
        .send_ecdh_init = 0,
        .idle_ms = 3000,
        .log_path = NULL,
    };
    for (int index = 1; index < argc; index++) {
        const char *arg = argv[index];
        if (strcmp(arg, "--host") == 0 && index + 1 < argc) {
            options.host = argv[++index];
        } else if (strcmp(arg, "--port") == 0 && index + 1 < argc) {
            options.port = parse_port(argv[++index]);
            if (options.port < 0) {
                usage(argv[0]);
                return 2;
            }
        } else if (strcmp(arg, "--mode") == 0 && index + 1 < argc) {
            const char *mode = argv[++index];
            if (strcmp(mode, "client") != 0) {
                fprintf(stderr, "error: only --mode client is supported\n");
                return 2;
            }
        } else if (strcmp(arg, "--send-banner") == 0) {
            options.send_banner = 1;
        } else if (strcmp(arg, "--send-kexinit") == 0) {
            options.send_kexinit = 1;
        } else if (strcmp(arg, "--send-kex-ecdh-init") == 0) {
            options.send_ecdh_init = 1;
        } else if (strcmp(arg, "--idle-ms") == 0 && index + 1 < argc) {
            options.idle_ms = atoi(argv[++index]);
            if (options.idle_ms < 0) {
                usage(argv[0]);
                return 2;
            }
        } else if (strcmp(arg, "--log") == 0 && index + 1 < argc) {
            options.log_path = argv[++index];
        } else if (strcmp(arg, "--help") == 0 || strcmp(arg, "-h") == 0) {
            usage(argv[0]);
            return 0;
        } else {
            fprintf(stderr, "error: unknown argument `%s`\n", arg);
            usage(argv[0]);
            return 2;
        }
    }
    clock_gettime(CLOCK_MONOTONIC, &start_time);
    if (options.log_path != NULL) {
        log_file = fopen(options.log_path, "w");
        if (log_file == NULL) {
            fprintf(stderr, "error: cannot open log %s: %s\n", options.log_path,
                    strerror(errno));
            return 2;
        }
    }
    emit("%06lld ms connect %s:%d", now_ms(), options.host, options.port);
    int fd = connect_timed(options.host, options.port);
    if (fd < 0) {
        if (log_file != NULL) {
            fclose(log_file);
        }
        return 2;
    }
    emit("%06lld ms connected fd=%d", now_ms(), fd);
    if (options.send_banner) {
        static const char banner[] = "SSH-2.0-FakePeer_1.0\r\n";
        send_bytes(fd, "banner", (const unsigned char *)banner, sizeof banner - 1);
    }
    unsigned char packet[4096];
    size_t kexinit_length = options.send_kexinit ? build_kexinit(packet, sizeof packet) : 0;
    struct reader reader;
    memset(&reader, 0, sizeof reader);
    reader.in_version = 1;
    int sent_kexinit = 0;
    int sent_ecdh = 0;
    int result = 0;
    for (;;) {
        struct pollfd pfd = {.fd = fd, .events = POLLIN};
        int ready = poll(&pfd, 1, options.idle_ms);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            emit("%06lld ms error: poll: %s", now_ms(), strerror(errno));
            result = 2;
            break;
        }
        if (ready == 0) {
            emit("%06lld ms idle %d ms, no bytes; total_read=%zu total_records=%zu",
                 now_ms(), options.idle_ms, reader.total, reader.records);
            break;
        }
        if (reader.length >= sizeof reader.buffer) {
            emit("%06lld ms error: internal buffer full", now_ms());
            result = 2;
            break;
        }
        ssize_t count = read(fd, reader.buffer + reader.length,
                             sizeof reader.buffer - reader.length);
        if (count == 0) {
            emit("%06lld ms eof after %zu bytes", now_ms(), reader.total);
            break;
        }
        if (count < 0) {
            if (errno == EINTR || errno == EAGAIN) {
                continue;
            }
            emit("%06lld ms error: read: %s", now_ms(), strerror(errno));
            result = 2;
            break;
        }
        reader.total += (size_t)count;
        reader.length += (size_t)count;
        char hex[3 * DUMP_BYTES + 8];
        char text[DUMP_BYTES + 8];
        hex_and_printable(reader.buffer + reader.length - (size_t)count, (size_t)count, hex,
                          sizeof hex, text, sizeof text);
        emit("%06lld ms read %zd bytes: %s |%s|", now_ms(), count, hex, text);
        size_t before = reader.length;
        parse_stream(&reader);
        (void)before;
        if (options.send_kexinit && !sent_kexinit && reader.saw_version) {
            send_bytes(fd, "KEXINIT record", packet, kexinit_length);
            sent_kexinit = 1;
        }
        if (options.send_ecdh_init && !sent_ecdh && reader.saw_kexinit) {
            unsigned char ecdh[128];
            size_t length = build_ecdh_init(ecdh, sizeof ecdh);
            send_bytes(fd, "KEX_ECDH_INIT record", ecdh, length);
            sent_ecdh = 1;
        }
    }
    emit("%06lld ms summary total_read=%zu total_records=%zu kexinit_sent=%d ecdh_init_sent=%d",
         now_ms(), reader.total, reader.records, sent_kexinit, sent_ecdh);
    close(fd);
    if (log_file != NULL) {
        fclose(log_file);
    }
    return result;
}
