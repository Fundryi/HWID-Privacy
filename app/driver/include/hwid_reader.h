#ifndef HWID_READER_ABI_H
#define HWID_READER_ABI_H

/* Wire contract: little endian, x64, natural alignment, no pointers or handles.
 * Version/size/reserved must be checked on BOTH sides. See README.md. */
#include <stdint.h>
#include <stddef.h>

#define HR_ABI_VERSION 1u
#define HR_MAX_SLOTS 32u
#define HR_MAX_SPD_BYTES 1024u
#define HR_DEVICE_TYPE 0x8000u
/* CTL_CODE(private device type, private function, METHOD_BUFFERED, FILE_ANY_ACCESS).
 * FILE_ANY_ACCESS does not bypass the SYSTEM/Administrators device DACL. */
#define HR_IOCTL(fn) ((HR_DEVICE_TYPE << 16) | ((fn) << 2))
#define HR_IOCTL_CAPABILITIES HR_IOCTL(0x800u)
#define HR_IOCTL_ENUMERATE_SPD HR_IOCTL(0x801u)
#define HR_IOCTL_READ_SPD HR_IOCTL(0x802u)
#define HR_IOCTL_RESERVED_PPIN HR_IOCTL(0x810u)
#define HR_IOCTL_RESERVED_DSN HR_IOCTL(0x811u)

#define HR_STATUS_OK 0u
#define HR_STATUS_UNSUPPORTED 1u
#define HR_STATUS_BUSY 2u
#define HR_STATUS_TIMEOUT 3u
#define HR_STATUS_IO_ERROR 4u
#define HR_STATUS_CANCELLED 5u
#define HR_STATUS_STALE_TOKEN 6u
#define HR_STATUS_NOT_PRESENT 7u
#define HR_FEATURE_SPD 1u
#define HR_FEATURE_PARTIAL_SPD 2u
#define HR_SPD_PARTIAL 1u
#define HR_BACKEND_NONE 0u
#define HR_BACKEND_INTEL_I801 1u
#define HR_BACKEND_AMD_FCH 2u
#define HR_GENERATION_DDR3 3u
#define HR_GENERATION_DDR4 4u

typedef struct HR_HEADER {
    uint32_t version;
    uint32_t size;
    uint32_t reserved[2];
} HR_HEADER;

typedef struct HR_CAPABILITIES {
    HR_HEADER header;
    uint32_t status;
    uint32_t features;
    uint32_t max_slots;
    uint32_t max_spd_bytes;
    uint32_t backend;
    uint32_t reserved;
} HR_CAPABILITIES;

typedef struct HR_SLOT {
    uint64_t token;             /* Opaque, valid only on this open and enumeration. */
    uint32_t generation;
    uint32_t controller;        /* Backend kind, never a PCI address. */
    uint32_t flags;
    uint32_t reserved;
} HR_SLOT;

typedef struct HR_ENUMERATE_REPLY {
    HR_HEADER header;
    uint32_t status;
    uint32_t count;
    HR_SLOT slots[HR_MAX_SLOTS];
} HR_ENUMERATE_REPLY;

typedef struct HR_READ_REQUEST {
    HR_HEADER header;
    uint64_t token;
} HR_READ_REQUEST;

typedef struct HR_READ_REPLY {
    HR_HEADER header;
    uint32_t status;
    uint32_t byte_count;
    uint64_t token;
    uint32_t generation;
    uint32_t flags;
    uint8_t bytes[HR_MAX_SPD_BYTES];
} HR_READ_REPLY;

/* Reserved numbers have no implementation, feature bit, or privileged primitive.
 * These are their fixed envelopes only; v1 always returns UNSUPPORTED. */
typedef struct HR_RESERVED_REPLY {
    HR_HEADER header;
    uint32_t status;
    uint32_t reserved;
} HR_RESERVED_REPLY;

_Static_assert(sizeof(HR_HEADER) == 16, "header ABI");
_Static_assert(sizeof(HR_CAPABILITIES) == 40, "capabilities ABI");
_Static_assert(sizeof(HR_SLOT) == 24, "slot ABI");
_Static_assert(sizeof(HR_ENUMERATE_REPLY) == 792, "enumeration ABI");
_Static_assert(sizeof(HR_READ_REQUEST) == 24, "read request ABI");
_Static_assert(sizeof(HR_READ_REPLY) == 1064, "read reply ABI");
_Static_assert(offsetof(HR_READ_REPLY, bytes) == 40, "SPD byte offset");
_Static_assert(sizeof(HR_RESERVED_REPLY) == 24, "reserved ABI");
#endif
