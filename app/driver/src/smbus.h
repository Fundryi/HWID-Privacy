#ifndef HWID_READER_SMBUS_H
#define HWID_READER_SMBUS_H
#include "protocol.h"

/* Only driver-internal fixed register offsets reach this glue, never ABI data. */
typedef struct HR_BUS {
    void *context;
    uint32_t kind;
    int poisoned;              /* A timed-out bus is never retried this D0 lifetime. */
    uint64_t deadline;
    uint8_t (*load)(void *, uint8_t);
    void (*store)(void *, uint8_t, uint8_t);
    uint64_t (*now)(void *);
    void (*delay)(void *);
    int (*cancelled)(void *);
} HR_BUS;

uint32_t HrSpdEnumerate(void *, HR_INTERNAL_SLOT *, uint32_t *);
uint32_t HrSpdRead(void *, HR_INTERNAL_SLOT, uint8_t *, uint32_t *);
/* Pure binding checks, shared by host checks and KMDF resource discovery. */
uint32_t HrPciFamily(const uint8_t config[256]);
uint32_t HrIntelBase(const uint8_t config[256]);
uint32_t HrAmdBase(uint32_t decode);
#endif
