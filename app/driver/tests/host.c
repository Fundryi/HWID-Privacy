/* No WDK or hardware access. Compiles the actual ABI dispatcher and bus engine. */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "smbus.h"

typedef union BUFFER {
    HR_CAPABILITIES caps;
    HR_ENUMERATE_REPLY enumeration;
    HR_READ_REPLY read;
    HR_RESERVED_REPLY reserved;
    uint8_t bytes[1080];
} BUFFER;

typedef struct FAKE_BUS {
    uint8_t regs[32];
    uint64_t clock;
    uint32_t kind;
    uint32_t transactions;
    uint32_t writes;
    int generation;
    int cancelled;
    int hung;
    int nack;
    int page;
    int cancel_after;
} FAKE_BUS;

static uint8_t FakeLoad(void *context, uint8_t offset)
{
    FAKE_BUS *fake = context;
    uint8_t old = fake->regs[offset];
    assert(offset < 32);
    if (offset == 0 && fake->kind == HR_BACKEND_INTEL_I801) fake->regs[0] |= 0x40;
    return old;
}

static void FakeStore(void *context, uint8_t offset, uint8_t value)
{
    FAKE_BUS *fake = context;
    uint8_t target;
    assert(offset == 0 || offset == 2 || offset == 3 || offset == 4 || offset == 8);
    ++fake->writes;
    if (offset == 0) { fake->regs[0] &= (uint8_t)~value; return; }
    if (offset == 8) {
        assert(fake->kind == HR_BACKEND_AMD_FCH);
        assert(value == 0x10 || value == 0x20);
        fake->regs[8] = value == 0x10 ? 0x10 : 0;
        return;
    }
    if (offset == 4) assert((value & 1) != 0); /* ALL bus operations are reads. */
    fake->regs[offset] = value;
    if (offset == 2 && value == 0x02) {
        fake->regs[0] = (uint8_t)((fake->regs[0] & 0x40) | 0x10);
        return;
    }
    if (offset != 2 || (value & 0x40) == 0) return;
    assert(value == 0x44 || value == 0x48);
    target = fake->regs[4] >> 1;
    assert(target == 0x36 || (target >= 0x50 && target <= 0x57));
    ++fake->transactions;
    fake->regs[2] &= (uint8_t)~0x40;
    fake->regs[0] &= 0x40;
    if (fake->hung) { fake->regs[0] |= 1; return; }
    if (fake->nack || (target != 0x50 && target != 0x36) || (target == 0x36 && fake->page != 0)) {
        fake->regs[0] |= 4;
        return;
    }
    fake->regs[0] |= 2;
    if (target == 0x36) fake->regs[5] = (uint8_t)fake->page;
    else if (fake->regs[3] == 2) fake->regs[5] = fake->generation == 4 ? 0x0c : 0x0b;
    else fake->regs[5] = (uint8_t)(fake->regs[3] ^ 0x5a);
}

static uint64_t FakeNow(void *context) { return ((FAKE_BUS *)context)->clock; }
static void FakeDelay(void *context) { ++((FAKE_BUS *)context)->clock; }
static int FakeCancelled(void *context)
{
    FAKE_BUS *fake = context;
    return fake->cancelled || (fake->cancel_after >= 0 &&
        fake->transactions >= (uint32_t)fake->cancel_after);
}

static void FakeInit(FAKE_BUS *fake, HR_BUS *bus, uint32_t kind, int generation)
{
    memset(fake, 0, sizeof(*fake));
    memset(bus, 0, sizeof(*bus));
    fake->kind = kind;
    fake->generation = generation;
    fake->cancel_after = -1;
    bus->kind = kind;
    bus->context = fake;
    bus->load = FakeLoad;
    bus->store = FakeStore;
    bus->now = FakeNow;
    bus->delay = FakeDelay;
    bus->cancelled = FakeCancelled;
}

static HR_HEADER Header(uint32_t size)
{
    HR_HEADER header = {HR_ABI_VERSION, 0, {0, 0}};
    header.size = size;
    return header;
}

static void AllZero(const void *buffer, size_t length)
{
    const uint8_t *bytes = buffer;
    size_t i;
    for (i = 0; i < length; ++i) assert(bytes[i] == 0);
}

static uint32_t TooMany(void *context, HR_INTERNAL_SLOT *slots, uint32_t *count)
{
    (void)context; (void)slots; *count = HR_MAX_SLOTS + 1;
    return HR_STATUS_OK;
}

static uint32_t FailedRead(void *context, HR_INTERNAL_SLOT slot, uint8_t *bytes, uint32_t *count)
{
    (void)context; (void)slot;
    memset(bytes, 0xa5, HR_MAX_SPD_BYTES);
    *count = HR_MAX_SPD_BYTES;
    return HR_STATUS_IO_ERROR;
}

static void Contract(void)
{
    HR_SESSION session, other;
    FAKE_BUS fake;
    HR_BUS bus;
    HR_BACKEND backend;
    BUFFER buffer;
    HR_HEADER input = Header(sizeof(input));
    HR_READ_REQUEST read = {0};
    uint64_t token;
    size_t written, required_input, required_output, n;
    uint32_t ioctl;
    HR_DISPATCH_RESULT result;
    FakeInit(&fake, &bus, HR_BACKEND_INTEL_I801, 3);
    backend.context = &bus; backend.kind = bus.kind; backend.epoch = 7;
    backend.enumerate = HrSpdEnumerate; backend.read = HrSpdRead;
    HrSessionInit(&session, 1);
    for (ioctl = HR_IOCTL_CAPABILITIES; ioctl <= HR_IOCTL_READ_SPD; ioctl += 4) {
        assert(HrIoctlSizes(ioctl, &required_input, &required_output));
        assert((ioctl & 3) == 0 && ((ioctl >> 14) & 3) == 0);
        for (n = 0; n < required_input; ++n) {
            assert(HrDispatch(&session, &backend, ioctl, &read, n,
                &buffer, sizeof(buffer), &written) == HrDispatchInvalid);
            assert(written == 0);
        }
        assert(HrDispatch(&session, &backend, ioctl, &read, required_input + 1,
            &buffer, sizeof(buffer), &written) == HrDispatchInvalid);
        assert(HrDispatch(&session, &backend, ioctl, &read, required_input,
            &buffer, required_output - 1, &written) == HrDispatchSmallOutput);
    }
    for (ioctl = 0; ioctl < 0x10000; ioctl += 4) {
        assert(HrDispatch(&session, &backend, ioctl, &input, sizeof(input),
            &buffer, sizeof(buffer), &written) == HrDispatchUnknown);
    }
    input.version = 2;
    assert(HrDispatch(&session, &backend, HR_IOCTL_CAPABILITIES, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchInvalid);
    input = Header(sizeof(input)); input.reserved[1] = 1;
    assert(HrDispatch(&session, &backend, HR_IOCTL_CAPABILITIES, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchInvalid);
    input = Header(sizeof(input)); input.size = 15;
    assert(HrDispatch(&session, &backend, HR_IOCTL_CAPABILITIES, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchInvalid);
    input = Header(sizeof(input));
    memset(&buffer, 0xa5, sizeof(buffer));
    assert(HrDispatch(&session, &backend, HR_IOCTL_CAPABILITIES, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(written == 40 && buffer.caps.features == 3 && buffer.caps.reserved == 0);
    assert(buffer.bytes[40] == 0xa5); /* Not returned: oversized output tail is untouched. */
    /* Real METHOD_BUFFERED aliasing, including header preservation. */
    memcpy(&buffer, &input, sizeof(input));
    assert(HrDispatch(&session, &backend, HR_IOCTL_ENUMERATE_SPD, &buffer, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(written == 792 && buffer.enumeration.status == 0 && buffer.enumeration.count == 1);
    token = buffer.enumeration.slots[0].token;
    assert(token != 0 && buffer.enumeration.slots[0].generation == 3);
    AllZero(&buffer.enumeration.slots[1], sizeof(HR_SLOT) * (HR_MAX_SLOTS - 1));
    read.header = Header(sizeof(read)); read.token = token;
    memcpy(&buffer, &read, sizeof(read));
    result = HrDispatch(&session, &backend, HR_IOCTL_READ_SPD, &buffer, sizeof(read),
        &buffer, sizeof(buffer), &written);
    assert(result == HrDispatchOk && written == 1064);
    assert(buffer.read.status == 0 && buffer.read.byte_count == 256 && buffer.read.token == token);
    assert(buffer.read.bytes[1] == (1 ^ 0x5a) && buffer.read.bytes[2] == 0x0b);
    AllZero(buffer.read.bytes + 256, HR_MAX_SPD_BYTES - 256);

    HrSessionInit(&other, 2);
    assert(HrDispatch(&other, &backend, HR_IOCTL_READ_SPD, &read, sizeof(read),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.read.status == HR_STATUS_STALE_TOKEN);
    assert(HrDispatch(&session, &backend, HR_IOCTL_ENUMERATE_SPD, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.enumeration.slots[0].token != token);
    assert(HrDispatch(&session, &backend, HR_IOCTL_READ_SPD, &read, sizeof(read),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.read.status == HR_STATUS_STALE_TOKEN);
    read.token = session.tokens[0];
    backend.read = FailedRead;
    assert(HrDispatch(&session, &backend, HR_IOCTL_READ_SPD, &read, sizeof(read),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.read.status == HR_STATUS_IO_ERROR && buffer.read.byte_count == 0);
    AllZero(buffer.read.bytes, sizeof(buffer.read.bytes));
    ++backend.epoch; /* Power/remove epoch invalidates tokens before any transaction. */
    assert(HrDispatch(&session, &backend, HR_IOCTL_READ_SPD, &read, sizeof(read),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.read.status == HR_STATUS_STALE_TOKEN && session.count == 0);
    backend.enumerate = TooMany;
    assert(HrDispatch(&session, &backend, HR_IOCTL_ENUMERATE_SPD, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.enumeration.status == HR_STATUS_IO_ERROR && buffer.enumeration.count == 0);
    AllZero(buffer.enumeration.slots, sizeof(buffer.enumeration.slots));
    for (ioctl = HR_IOCTL_RESERVED_PPIN; ioctl <= HR_IOCTL_RESERVED_DSN; ioctl += 4) {
        assert(HrDispatch(&session, &backend, ioctl, &input, sizeof(input),
            &buffer, sizeof(buffer), &written) == HrDispatchOk);
        assert(written == 24 && buffer.reserved.status == HR_STATUS_UNSUPPORTED && buffer.reserved.reserved == 0);
    }
    backend.kind = HR_BACKEND_NONE;
    assert(HrDispatch(&session, &backend, HR_IOCTL_ENUMERATE_SPD, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    assert(buffer.enumeration.status == HR_STATUS_UNSUPPORTED);
}

static void BusChecks(uint32_t kind)
{
    FAKE_BUS fake;
    HR_BUS bus;
    HR_INTERNAL_SLOT slots[HR_MAX_SLOTS] = {0};
    uint32_t count, status, writes;
    uint8_t bytes[HR_MAX_SPD_BYTES] = {0};
    FakeInit(&fake, &bus, kind, 4);
    assert(HrSpdEnumerate(&bus, slots, &count) == 0 && count == 1 && slots[0].generation == 4);
    assert(HrSpdRead(&bus, slots[0], bytes, &count) == 0 && count == 256);
    fake.page = 1;
    assert(HrSpdRead(&bus, slots[0], bytes, &count) == HR_STATUS_UNSUPPORTED && count == 0);
    AllZero(bytes, sizeof(bytes));
    FakeInit(&fake, &bus, kind, 3);
    fake.cancelled = 1;
    assert(HrSpdEnumerate(&bus, slots, &count) == HR_STATUS_CANCELLED && fake.writes == 0);
    FakeInit(&fake, &bus, kind, 3);
    if (kind == HR_BACKEND_INTEL_I801) fake.regs[0] = 0x40;
    else fake.regs[8] = 0x40;
    assert(HrSpdEnumerate(&bus, slots, &count) == HR_STATUS_BUSY && fake.writes == 0);
    FakeInit(&fake, &bus, kind, 3);
    fake.hung = 1;
    status = HrSpdEnumerate(&bus, slots, &count);
    assert(status == HR_STATUS_TIMEOUT && count == 0 && bus.poisoned);
    assert(fake.clock <= 40);
    writes = fake.writes;
    assert(HrSpdEnumerate(&bus, slots, &count) == HR_STATUS_IO_ERROR && fake.writes == writes);
    FakeInit(&fake, &bus, kind, 3);
    slots[0].target = 0x50; slots[0].generation = 3;
    fake.cancel_after = 7;
    memset(bytes, 0xa5, sizeof(bytes));
    assert(HrSpdRead(&bus, slots[0], bytes, &count) == HR_STATUS_CANCELLED && count == 0);
    AllZero(bytes, sizeof(bytes));
    FakeInit(&fake, &bus, kind, 3);
    fake.nack = 1;
    assert(HrSpdEnumerate(&bus, slots, &count) == HR_STATUS_UNSUPPORTED && count == 0);
    assert(fake.transactions == 1); /* Ambiguous page state stops before SPD targets. */
    slots[0].target = 0x49;
    assert(HrSpdRead(&bus, slots[0], bytes, &count) == HR_STATUS_UNSUPPORTED);
    assert((fake.regs[0] & 0x40) == 0 && fake.regs[8] == 0);
}

static void Discovery(void)
{
    uint8_t config[256] = {0};
    config[0] = 0x86; config[1] = 0x80; config[2] = 0x23; config[3] = 0xa1;
    config[0x0b] = 0x0c; config[0x0a] = 5;
    config[4] = 1; config[0x40] = 1;
    config[0x20] = 0x21; config[0x21] = 0x0b; /* Fabricated BAR, not a live capture. */
    assert(HrPciFamily(config) == HR_BACKEND_INTEL_I801 && HrIntelBase(config) == 0x0b20);
    config[0x40] = 5; assert(HrIntelBase(config) == 0); /* I2C mode forbidden. */
    config[0x40] = 1; config[0x22] = 1; assert(HrIntelBase(config) == 0);
    config[0] = 0x22; config[1] = 0x10; config[2] = 0x0b; config[3] = 0x79;
    config[8] = 0x51;
    assert(HrPciFamily(config) == HR_BACKEND_AMD_FCH);
    config[8] = 0x50; assert(HrPciFamily(config) == HR_BACKEND_NONE);
    assert(HrAmdBase(0x0b10) == 0x0b00);
    assert(HrAmdBase(0x0b00) == 0);
    assert(HrAmdBase(0x080b10) == 0); /* Never switch mux ports. */
}

static void WireFixture(void)
{
    FAKE_BUS fake;
    HR_BUS bus;
    HR_SESSION session;
    HR_BACKEND backend;
    BUFFER buffer;
    HR_HEADER input = Header(sizeof(input));
    HR_READ_REQUEST read = {0};
    size_t written, i;
    FakeInit(&fake, &bus, HR_BACKEND_INTEL_I801, 3);
    backend.context = &bus; backend.kind = bus.kind; backend.epoch = 1;
    backend.enumerate = HrSpdEnumerate; backend.read = HrSpdRead;
    HrSessionInit(&session, 1);
    assert(HrDispatch(&session, &backend, HR_IOCTL_ENUMERATE_SPD, &input, sizeof(input),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    read.header = Header(sizeof(read)); read.token = session.tokens[0];
    memcpy(&buffer, &read, sizeof(read));
    assert(HrDispatch(&session, &backend, HR_IOCTL_READ_SPD, &buffer, sizeof(read),
        &buffer, sizeof(buffer), &written) == HrDispatchOk);
    for (i = 0; i < written; ++i) printf("%02x", buffer.bytes[i]);
    puts("");
}

int main(int argc, char **argv)
{
    if (argc == 2 && strcmp(argv[1], "--wire") == 0) { WireFixture(); return 0; }
    Contract();
    BusChecks(HR_BACKEND_INTEL_I801);
    BusChecks(HR_BACKEND_AMD_FCH);
    Discovery();
    puts("C host checks passed: actual dispatcher, aliased ABI round-trip, validation, tokens, zeroing, controller binding, read-only bus operations, arbitration, cancellation and deadlines.");
    return 0;
}
