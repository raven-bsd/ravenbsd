// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K375 fake SSH peer tracer. This is host-side test tooling; it is never linked into the product
// and never staged in an image. The SSH version exchange and binary packet framing are taken from
// RFC 4253 section 4.2 and RFC 4251 section 6; no donor source is copied.
//
// The point of the tool is byte-exact evidence: every read and write is timestamped with
// CLOCK_MONOTONIC, dumped as hex and printable text, and flushed immediately. A peer that sends
// nothing shows a zero-byte idle timeout instead of a client-side "operation timed out".

#define _POSIX_C_SOURCE 200809L

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <netdb.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define SSH_MSG_DISCONNECT 1
#define SSH_MSG_IGNORE 2
#define SSH_MSG_UNIMPLEMENTED 3
#define SSH_MSG_DEBUG 4
#define SSH_MSG_SERVICE_REQUEST 5
#define SSH_MSG_SERVICE_ACCEPT 6
#define SSH_MSG_KEXINIT 20
#define SSH_MSG_NEWKEYS 21
#define SSH_MSG_KEX_ECDH_REPLY 31

#define MAX_PACKET 35000
#define READ_CHUNK 8192
#define ANALYZER_CAPACITY (MAX_PACKET * 2)
#define MAX_NAMELISTS 10
#define MAX_NAMELIST_TEXT 1024
#define CONNECT_TIMEOUT_MS 2000

static FILE *log_file;
static double start_ms;

static double monotonic_ms(void) {
    struct timespec now;

    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0.0;
    }
    return (double)now.tv_sec * 1000.0 + (double)now.tv_nsec / 1000000.0;
}

static void log_prefix(const char *kind) {
    fprintf(log_file, "[t=%9.3fms] %s ", monotonic_ms() - start_ms, kind);
}

static void log_line(const char *kind, const char *format, ...) {
    va_list arguments;

    log_prefix(kind);
    va_start(arguments, format);
    vfprintf(log_file, format, arguments);
    va_end(arguments);
    fputc('\n', log_file);
    fflush(log_file);
}

static void log_hex(const char *kind, const uint8_t *data, size_t length) {
    size_t offset;

    log_prefix(kind);
    fprintf(log_file, "%zu bytes\n", length);
    for (offset = 0; offset < length; offset += 16) {
        size_t index;

        fprintf(log_file, "  %04zx  ", offset);
        for (index = 0; index < 16; index++) {
            if (offset + index < length) {
                fprintf(log_file, "%02x ", data[offset + index]);
            } else {
                fputs("   ", log_file);
            }
            if (index == 7) {
                fputc(' ', log_file);
            }
        }
        fputs(" |", log_file);
        for (index = 0; index < 16 && offset + index < length; index++) {
            uint8_t byte = data[offset + index];

            fputc(byte >= 0x20 && byte <= 0x7e ? (int)byte : '.', log_file);
        }
        fputs("|\n", log_file);
    }
    fflush(log_file);
}

static void record_write(const char *what, const uint8_t *data, size_t length, int result,
                         int error) {
    if (result < 0) {
        log_line("write-error", "%s length=%zu errno=%d (%s)", what, length, error,
                 strerror(error));
        return;
    }
    log_line("write", "%s length=%zu", what, length);
    log_hex("write-bytes", data, length);
}

static int write_all(int fd, const uint8_t *data, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t written = send(fd, data + done, length - done, 0);

        if (written < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)written;
    }
    return 0;
}

static uint32_t read_be32(const uint8_t *data) {
    return ((uint32_t)data[0] << 24) | ((uint32_t)data[1] << 16) | ((uint32_t)data[2] << 8) |
           (uint32_t)data[3];
}

static size_t put_be32(uint8_t *data, uint32_t value) {
    data[0] = (uint8_t)(value >> 24);
    data[1] = (uint8_t)(value >> 16);
    data[2] = (uint8_t)(value >> 8);
    data[3] = (uint8_t)value;
    return 4;
}

static size_t put_name_list(uint8_t *data, const char *text) {
    size_t length = strlen(text);

    put_be32(data, (uint32_t)length);
    memcpy(data + 4, text, length);
    return 4 + length;
}

static void fill_random(uint8_t *data, size_t length) {
    int fd = open("/dev/urandom", O_RDONLY);
    size_t done = 0;

    if (fd < 0) {
        memset(data, 0, length);
        return;
    }
    while (done < length) {
        ssize_t got = read(fd, data + done, length - done);

        if (got <= 0) {
            memset(data + done, 0, length - done);
            break;
        }
        done += (size_t)got;
    }
    close(fd);
}

// Build one unencrypted SSH_MSG_KEXINIT. The packet framing is RFC 4251 section 6: uint32
// packet_length, one padding_length byte, the payload, then padding. For an unencrypted packet the
// block size is 8 and the padding is 4..255 bytes.
static size_t build_kexinit(uint8_t *out, size_t capacity) {
    static const char *kex_algorithms =
        "curve25519-sha256,curve25519-sha256@libssh.org,ecdh-sha2-nistp256,"
        "ecdh-sha2-nistp384,ecdh-sha2-nistp521,diffie-hellman-group14-sha256,"
        "diffie-hellman-group16-sha512";
    static const char *host_key_algorithms =
        "ssh-ed25519,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256,ssh-rsa";
    static const char *encryption_algorithms =
        "chacha20-poly1305@openssh.com,aes128-ctr,aes192-ctr,aes256-ctr,"
        "aes128-gcm@openssh.com,aes256-gcm@openssh.com";
    static const char *mac_algorithms =
        "hmac-sha2-256-etm@openssh.com,hmac-sha2-512-etm@openssh.com,hmac-sha2-256,"
        "hmac-sha2-512,hmac-sha1";
    uint8_t payload[2048];
    size_t length = 0;
    size_t padding;
    size_t packet_length;
    size_t total;
    uint8_t cookie[16];

    payload[length++] = SSH_MSG_KEXINIT;
    fill_random(cookie, sizeof(cookie));
    memcpy(payload + length, cookie, sizeof(cookie));
    length += sizeof(cookie);
    length += put_name_list(payload + length, kex_algorithms);
    length += put_name_list(payload + length, host_key_algorithms);
    length += put_name_list(payload + length, encryption_algorithms);
    length += put_name_list(payload + length, encryption_algorithms);
    length += put_name_list(payload + length, mac_algorithms);
    length += put_name_list(payload + length, mac_algorithms);
    length += put_name_list(payload + length, "none");
    length += put_name_list(payload + length, "none");
    length += put_name_list(payload + length, "");
    length += put_name_list(payload + length, "");
    payload[length++] = 0; // first_kex_packet_follows = false
    length += put_be32(payload + length, 0);

    padding = 8 - ((4 + 1 + length) % 8);
    if (padding < 4) {
        padding += 8;
    }
    packet_length = 1 + length + padding;
    total = 4 + packet_length;
    if (total > capacity) {
        return 0;
    }
    put_be32(out, (uint32_t)packet_length);
    out[4] = (uint8_t)padding;
    memcpy(out + 5, payload, length);
    fill_random(out + 5 + length, padding);
    return total;
}

static const char *message_name(uint8_t message) {
    switch (message) {
    case SSH_MSG_DISCONNECT:
        return "SSH2_MSG_DISCONNECT";
    case SSH_MSG_IGNORE:
        return "SSH2_MSG_IGNORE";
    case SSH_MSG_UNIMPLEMENTED:
        return "SSH2_MSG_UNIMPLEMENTED";
    case SSH_MSG_DEBUG:
        return "SSH2_MSG_DEBUG";
    case SSH_MSG_SERVICE_REQUEST:
        return "SSH2_MSG_SERVICE_REQUEST";
    case SSH_MSG_SERVICE_ACCEPT:
        return "SSH2_MSG_SERVICE_ACCEPT";
    case SSH_MSG_KEXINIT:
        return "SSH2_MSG_KEXINIT";
    case SSH_MSG_NEWKEYS:
        return "SSH2_MSG_NEWKEYS";
    case SSH_MSG_KEX_ECDH_REPLY:
        return "SSH2_MSG_KEX_ECDH_REPLY";
    default:
        return "unknown";
    }
}

struct analyzer {
    uint8_t data[ANALYZER_CAPACITY];
    size_t length;
    uint8_t banner[513];
    size_t banner_length;
    int banner_done;
    unsigned packets;
    unsigned kexinit_seen;
    unsigned ecdh_reply_seen;
    unsigned newkeys_seen;
    unsigned malformed;
};

static void parse_name_lists(const uint8_t *payload, size_t length) {
    size_t offset = 17; // message byte plus 16-byte cookie
    unsigned index;

    for (index = 0; index < MAX_NAMELISTS && offset + 4 <= length; index++) {
        uint32_t name_length = read_be32(payload + offset);
        char text[MAX_NAMELIST_TEXT];
        size_t copy;

        offset += 4;
        if (name_length > length - offset) {
            log_line("kexinit-error", "name-list %u length=%u exceeds payload", index,
                     name_length);
            return;
        }
        copy = name_length < sizeof(text) - 1 ? name_length : sizeof(text) - 1;
        memcpy(text, payload + offset, copy);
        text[copy] = '\0';
        log_line("kexinit", "name-list[%u] length=%u value=\"%s\"", index, name_length, text);
        offset += name_length;
    }
}

static void analyze_packets(struct analyzer *analyzer) {
    while (analyzer->length >= 4) {
        uint32_t packet_length = read_be32(analyzer->data);
        uint8_t padding;
        size_t payload_length;
        uint8_t message;

        if (packet_length < 5 || packet_length > MAX_PACKET) {
            log_line("packet-invalid", "packet_length=%u buffered=%zu", packet_length,
                     analyzer->length);
            analyzer->malformed++;
            analyzer->length = 0;
            return;
        }
        if (analyzer->length < 4u + packet_length) {
            return;
        }
        padding = analyzer->data[4];
        if (padding < 4 || (size_t)padding + 1 > packet_length) {
            log_line("packet-invalid", "packet_length=%u padding_length=%u", packet_length,
                     padding);
            analyzer->malformed++;
            analyzer->length = 0;
            return;
        }
        payload_length = packet_length - 1u - padding;
        if (payload_length == 0) {
            log_line("packet-invalid", "packet_length=%u padding_length=%u empty payload",
                     packet_length, padding);
            analyzer->malformed++;
            analyzer->length = 0;
            return;
        }
        message = analyzer->data[5];
        log_line("packet", "length=%u padding=%u payload=%zu aligned8=%s msg=%u (%s)",
                 packet_length, padding, payload_length,
                 ((4u + packet_length) % 8 == 0) ? "yes" : "no", message,
                 message_name(message));
        log_hex("packet-bytes", analyzer->data, 4u + packet_length);
        analyzer->packets++;
        if (message == SSH_MSG_KEXINIT) {
            analyzer->kexinit_seen++;
            parse_name_lists(analyzer->data + 5, payload_length);
        } else if (message == SSH_MSG_KEX_ECDH_REPLY) {
            analyzer->ecdh_reply_seen++;
        } else if (message == SSH_MSG_NEWKEYS) {
            analyzer->newkeys_seen++;
        }
        memmove(analyzer->data, analyzer->data + 4u + packet_length,
                analyzer->length - 4u - packet_length);
        analyzer->length -= 4u + packet_length;
    }
}

static void analyze(struct analyzer *analyzer, const uint8_t *data, size_t length) {
    if (!analyzer->banner_done) {
        while (length > 0) {
            uint8_t byte = *data++;

            length--;
            if (analyzer->banner_length < sizeof(analyzer->banner)) {
                analyzer->banner[analyzer->banner_length++] = byte;
            }
            if (byte == '\n') {
                size_t copy = analyzer->banner_length;

                while (copy > 0 && (analyzer->banner[copy - 1] == '\n' ||
                                    analyzer->banner[copy - 1] == '\r')) {
                    copy--;
                }
                analyzer->banner[copy] = '\0';
                log_line("banner", "length=%zu value=\"%s\"", analyzer->banner_length,
                         (const char *)analyzer->banner);
                analyzer->banner_done = 1;
                analyzer->length = 0;
                break;
            }
        }
        if (!analyzer->banner_done) {
            return;
        }
    }
    if (length > sizeof(analyzer->data) - analyzer->length) {
        analyzer->length = 0;
    }
    memcpy(analyzer->data + analyzer->length, data, length);
    analyzer->length += length;
    analyze_packets(analyzer);
}

// Build one unencrypted SSH_MSG_KEX_ECDH_INIT. The 32-byte client ephemeral public key is
// random: curve25519 accepts any 32-byte string as a u-coordinate, and the peer never completes
// the key exchange. This is what makes the server's KEX_ECDH_REPLY observable; without a client
// ECDH_INIT a correct SSH server has nothing to reply to.
static size_t build_ecdh_init(uint8_t *out, size_t capacity) {
    uint8_t payload[37];
    size_t padding;
    size_t packet_length;

    payload[0] = 30; // SSH_MSG_KEX_ECDH_INIT
    put_be32(payload + 1, 32);
    fill_random(payload + 5, 32);
    padding = 8 - ((4 + 1 + sizeof(payload)) % 8);
    if (padding < 4) {
        padding += 8;
    }
    packet_length = 1 + sizeof(payload) + padding;
    if (4 + packet_length > capacity) {
        return 0;
    }
    put_be32(out, (uint32_t)packet_length);
    out[4] = (uint8_t)padding;
    memcpy(out + 5, payload, sizeof(payload));
    fill_random(out + 5 + sizeof(payload), padding);
    return 4 + packet_length;
}

static int send_ecdh_init_packet(int fd, size_t *total_written) {
    uint8_t packet[128];
    size_t length = build_ecdh_init(packet, sizeof(packet));

    if (length == 0) {
        log_line("error", "cannot build KEX_ECDH_INIT");
        return 1;
    }
    if (write_all(fd, packet, length) != 0) {
        record_write("client-ecdh-init", packet, length, -1, errno);
        return 1;
    }
    record_write("client-ecdh-init", packet, length, 0, 0);
    *total_written += length;
    return 0;
}

static int send_kexinit_packet(int fd, size_t *total_written) {
    uint8_t kexinit[1024];
    size_t length = build_kexinit(kexinit, sizeof(kexinit));

    if (length == 0) {
        log_line("error", "cannot build KEXINIT");
        return 1;
    }
    if (write_all(fd, kexinit, length) != 0) {
        record_write("client-kexinit", kexinit, length, -1, errno);
        return 1;
    }
    record_write("client-kexinit", kexinit, length, 0, 0);
    *total_written += length;
    return 0;
}

static int connect_to(const char *host, const char *port) {
    struct addrinfo hints;
    struct addrinfo *addresses = NULL;
    struct addrinfo *address;
    int fd = -1;
    int error = 0;

    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    error = getaddrinfo(host, port, &hints, &addresses);
    if (error != 0) {
        log_line("connect-error", "getaddrinfo(%s,%s): %s", host, port, gai_strerror(error));
        return -2;
    }
    for (address = addresses; address != NULL; address = address->ai_next) {
        int flags;
        struct pollfd pollfd;

        fd = socket(address->ai_family, address->ai_socktype, address->ai_protocol);
        if (fd < 0) {
            continue;
        }
        flags = fcntl(fd, F_GETFL, 0);
        if (flags >= 0) {
            fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
        error = connect(fd, address->ai_addr, address->ai_addrlen);
        if (error != 0 && errno == EINPROGRESS) {
            int socket_error = 0;
            socklen_t socket_error_length = sizeof(socket_error);

            pollfd.fd = fd;
            pollfd.events = POLLOUT;
            pollfd.revents = 0;
            error = poll(&pollfd, 1, CONNECT_TIMEOUT_MS);
            if (error == 1) {
                error = 0;
                if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &socket_error,
                               &socket_error_length) == 0) {
                    error = socket_error;
                } else {
                    error = errno;
                }
            } else if (error == 0) {
                error = ETIMEDOUT;
            } else {
                error = errno;
            }
        }
        if (error == 0) {
            if (flags >= 0) {
                fcntl(fd, F_SETFL, flags);
            }
            break;
        }
        close(fd);
        fd = -1;
    }
    freeaddrinfo(addresses);
    if (fd < 0) {
        log_line("connect-error", "cannot connect to %s:%s: %s", host, port, strerror(error));
        return -2;
    }
    log_line("connect", "connected to %s:%s fd=%d", host, port, fd);
    return fd;
}

static void usage(const char *program) {
    fprintf(stderr,
            "usage: %s --host HOST --port PORT --mode client [--send-banner] "
            "[--send-kexinit] [--idle-ms N] [--log FILE]\n",
            program);
}

int main(int argc, char **argv) {
    static const struct option options[] = {
        {"host", required_argument, NULL, 'H'},
        {"port", required_argument, NULL, 'p'},
        {"mode", required_argument, NULL, 'm'},
        {"send-banner", no_argument, NULL, 'b'},
        {"send-kexinit", no_argument, NULL, 'k'},
        {"idle-ms", required_argument, NULL, 'i'},
        {"post-idle-ms", required_argument, NULL, 'I'},
        {"log", required_argument, NULL, 'l'},
        {"help", no_argument, NULL, 'h'},
        {NULL, 0, NULL, 0},
    };
    const char *host = "127.0.0.1";
    const char *port = "2222";
    const char *mode = "client";
    const char *log_path = NULL;
    int send_banner = 0;
    int send_kexinit = 0;
    int kexinit_sent = 0;
    int ecdh_sent = 0;
    int idle_ms = 5000;
    int post_idle_ms = 2000;
    int current_idle;
    int option;
    int fd;
    int status = 0;
    struct analyzer analyzer;
    size_t total_read = 0;
    size_t total_written = 0;
    double last_data = 0.0;

    start_ms = monotonic_ms();
    while ((option = getopt_long(argc, argv, "H:p:m:bki:I:l:h", options, NULL)) != -1) {
        switch (option) {
        case 'H':
            host = optarg;
            break;
        case 'p':
            port = optarg;
            break;
        case 'm':
            mode = optarg;
            break;
        case 'b':
            send_banner = 1;
            break;
        case 'k':
            send_kexinit = 1;
            break;
        case 'i':
            idle_ms = atoi(optarg);
            break;
        case 'I':
            post_idle_ms = atoi(optarg);
            break;
        case 'l':
            log_path = optarg;
            break;
        case 'h':
            usage(argv[0]);
            return 0;
        default:
            usage(argv[0]);
            return 1;
        }
    }
    if (strcmp(mode, "client") != 0) {
        fprintf(stderr, "%s: only --mode client is implemented\n", argv[0]);
        return 1;
    }
    if (idle_ms <= 0 || post_idle_ms < 0) {
        fprintf(stderr, "%s: --idle-ms must be positive and --post-idle-ms non-negative\n",
                argv[0]);
        return 1;
    }
    log_file = stdout;
    if (log_path != NULL) {
        log_file = fopen(log_path, "w");
        if (log_file == NULL) {
            fprintf(stderr, "%s: cannot open %s: %s\n", argv[0], log_path, strerror(errno));
            return 1;
        }
    }
    setvbuf(log_file, NULL, _IONBF, 0);
    signal(SIGPIPE, SIG_IGN);
    memset(&analyzer, 0, sizeof(analyzer));

    log_line("start",
             "host=%s port=%s mode=%s send_banner=%d send_kexinit=%d idle_ms=%d post_idle_ms=%d",
             host, port, mode, send_banner, send_kexinit, idle_ms, post_idle_ms);
    fd = connect_to(host, port);
    if (fd < 0) {
        status = 2;
        goto done;
    }
    if (send_banner) {
        static const uint8_t banner[] = "SSH-2.0-FakePeer_1.0\r\n";

        if (write_all(fd, banner, sizeof(banner) - 1) != 0) {
            record_write("client-banner", banner, sizeof(banner) - 1, -1, errno);
            status = 1;
            goto done;
        }
        record_write("client-banner", banner, sizeof(banner) - 1, 0, 0);
        total_written += sizeof(banner) - 1;
    }
    if (send_kexinit && !send_banner) {
        if (send_kexinit_packet(fd, &total_written) != 0) {
            status = 1;
            goto done;
        }
        kexinit_sent = 1;
    }

    last_data = monotonic_ms();
    current_idle = idle_ms;
    for (;;) {
        struct pollfd pollfd;
        int remaining = current_idle - (int)(monotonic_ms() - last_data);
        int ready;

        if (remaining <= 0) {
            log_line("idle-timeout", "no data for %dms (total_read=%zu)", current_idle,
                     total_read);
            break;
        }
        pollfd.fd = fd;
        pollfd.events = POLLIN;
        pollfd.revents = 0;
        ready = poll(&pollfd, 1, remaining);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            log_line("read-error", "poll: %s", strerror(errno));
            status = 1;
            break;
        }
        if (ready == 0) {
            log_line("idle-timeout", "no data for %dms (total_read=%zu)", current_idle,
                     total_read);
            break;
        }
        if ((pollfd.revents & (POLLIN | POLLHUP | POLLERR)) != 0) {
            uint8_t buffer[READ_CHUNK];
            ssize_t got = recv(fd, buffer, sizeof(buffer), 0);

            if (got > 0) {
                last_data = monotonic_ms();
                total_read += (size_t)got;
                log_line("read", "%zd bytes", got);
                log_hex("read-bytes", buffer, (size_t)got);
                analyze(&analyzer, buffer, (size_t)got);
                if (send_kexinit && !kexinit_sent && analyzer.banner_done) {
                    if (send_kexinit_packet(fd, &total_written) != 0) {
                        status = 1;
                        break;
                    }
                    kexinit_sent = 1;
                }
                if (send_kexinit && kexinit_sent && !ecdh_sent && analyzer.kexinit_seen > 0) {
                    if (send_ecdh_init_packet(fd, &total_written) != 0) {
                        status = 1;
                        break;
                    }
                    ecdh_sent = 1;
                    log_line("kex",
                             "curve25519-sha256 selected from the client list; sent "
                             "KEX_ECDH_INIT");
                }
                if (analyzer.newkeys_seen > 0 && current_idle != post_idle_ms) {
                    log_line("handshake",
                             "NEWKEYS received; observing close for %dms", post_idle_ms);
                    current_idle = post_idle_ms;
                    last_data = monotonic_ms();
                    if (post_idle_ms == 0) {
                        break;
                    }
                }
                continue;
            }
            if (got == 0) {
                log_line("eof", "peer closed after total_read=%zu", total_read);
                break;
            }
            if (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK) {
                continue;
            }
            log_line("read-error", "recv: %s", strerror(errno));
            break;
        }
    }

done:
    if (fd >= 0 && total_read == 0 && status == 0) {
        // A connected peer that sent nothing is "not ready yet", not a successful capture.
        status = 3;
    }
    if (log_file != NULL) {
        log_line("summary",
                 "read=%zu write=%zu packets=%u kexinit=%u ecdh_reply=%u newkeys=%u malformed=%u",
                 total_read, total_written, analyzer.packets, analyzer.kexinit_seen,
                 analyzer.ecdh_reply_seen, analyzer.newkeys_seen, analyzer.malformed);
        if (analyzer.length > 0) {
            log_line("partial", "%zu trailing bytes not forming a packet", analyzer.length);
        }
        if (log_path != NULL) {
            fclose(log_file);
        }
    }
    if (fd >= 0) {
        close(fd);
    }
    return status;
}
