#ifndef HWID_READER_PROTOCOL_H
#define HWID_READER_PROTOCOL_H
#include "hwid_reader.h"

/* Internal slots contain driver-owned targets. They are never copied to the ABI. */
typedef struct HR_INTERNAL_SLOT {
    uint8_t target;
    uint8_t generation;
} HR_INTERNAL_SLOT;

typedef struct HR_SESSION {
    uint64_t epoch;
    uint32_t identity;
    uint32_t sequence;
    uint32_t count;
    uint64_t tokens[HR_MAX_SLOTS];
    HR_INTERNAL_SLOT slots[HR_MAX_SLOTS];
} HR_SESSION;

typedef struct HR_BACKEND {
    void *context;
    uint32_t kind;
    uint64_t epoch;
    uint32_t (*enumerate)(void *, HR_INTERNAL_SLOT *, uint32_t *);
    uint32_t (*read)(void *, HR_INTERNAL_SLOT, uint8_t *, uint32_t *);
} HR_BACKEND;

typedef enum HR_DISPATCH_RESULT {
    HrDispatchOk,
    HrDispatchInvalid,
    HrDispatchSmallOutput,
    HrDispatchUnknown
} HR_DISPATCH_RESULT;

void HrSessionInit(HR_SESSION *session, uint32_t identity);
void HrSessionInvalidate(HR_SESSION *session);
int HrIoctlSizes(uint32_t ioctl, size_t *input, size_t *output);
/* Input/output can alias, as they do in METHOD_BUFFERED. */
HR_DISPATCH_RESULT HrDispatch(HR_SESSION *, const HR_BACKEND *, uint32_t,
    const void *, size_t, void *, size_t, size_t *);
#endif
