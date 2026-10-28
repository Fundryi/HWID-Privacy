#include "smbus.h"
#include <string.h>

static uint16_t HrU16(const uint8_t *p)
{
    return (uint16_t)((uint16_t)p[0] | ((uint16_t)p[1] << 8));
}

uint32_t HrPciFamily(const uint8_t config[256])
{
    uint16_t vendor = HrU16(config), device = HrU16(config + 2);
    if (config[0x0b] != 0x0c || config[0x0a] != 0x05 || config[0x09] != 0 ||
        (config[0x0e] & 0x7f) != 0) return HR_BACKEND_NONE;
    if (vendor == 0x8086 && (device == 0xa123 || device == 0x9d23 || device == 0xa2a3)) {
        return HR_BACKEND_INTEL_I801;
    }
    /* Modern FCH only; earlier SB800 indexed PM access is deliberately absent. */
    if (vendor == 0x1022 && device == 0x790b && config[0x08] >= 0x51) {
        return HR_BACKEND_AMD_FCH;
    }
    return HR_BACKEND_NONE;
}

uint32_t HrIntelBase(const uint8_t config[256])
{
    uint32_t bar;
    if (HrPciFamily(config) != HR_BACKEND_INTEL_I801 || (config[4] & 1) == 0 ||
        (config[0x40] & 7) != 1) return 0; /* Enabled SMBus, neither SMI nor I2C mode. */
    bar = (uint32_t)HrU16(config + 0x20) | ((uint32_t)HrU16(config + 0x22) << 16);
    if ((bar & 1) == 0 || (bar & 0xffff0000u) != 0) return 0;
    return bar & 0xffe0u;
}

uint32_t HrAmdBase(uint32_t decode)
{
    /* FCH PM DecodeEn: enable bit 4, base [15:8], port mux [20:19].
     * Never enable decode or change the port mux. Port zero only. */
    if ((decode & 0x10) == 0 || (decode & 0x180000) != 0) return 0;
    return decode & 0xff00u;
}

static uint32_t HrStop(HR_BUS *bus)
{
    if (bus->cancelled(bus->context)) return HR_STATUS_CANCELLED;
    if (bus->now(bus->context) >= bus->deadline) return HR_STATUS_TIMEOUT;
    return HR_STATUS_OK;
}

static void HrRelease(HR_BUS *bus)
{
    if (bus->kind == HR_BACKEND_INTEL_I801) {
        bus->store(bus->context, 0, 0x40); /* Release our read-acquired INUSE semaphore. */
    } else {
        bus->store(bus->context, 8, 0x20); /* Release our FCH host semaphore. */
    }
}

static uint32_t HrClaim(HR_BUS *bus)
{
    uint8_t value;
    uint64_t end = bus->now(bus->context) + 20;
    uint32_t stop = HrStop(bus);
    if (stop != HR_STATUS_OK) return stop;
    if (bus->poisoned) return HR_STATUS_IO_ERROR;
    if (bus->kind == HR_BACKEND_INTEL_I801) {
        /* Reading HSTS acquires INUSE if it was clear. Never clear another owner. */
        value = bus->load(bus->context, 0);
        if ((value & 0x40) != 0) return HR_STATUS_BUSY;
        if ((value & 0xbf) != 0 || (bus->load(bus->context, 0x0d) & 1) != 0) {
            HrRelease(bus);
            return HR_STATUS_BUSY;
        }
        return HR_STATUS_OK;
    }
    if (bus->kind != HR_BACKEND_AMD_FCH) return HR_STATUS_UNSUPPORTED;
    value = bus->load(bus->context, 8);
    /* Refuse EC ownership, existing host ownership or pending requests. */
    if ((value & 0x50) != 0) return HR_STATUS_BUSY;
    bus->store(bus->context, 8, 0x10);
    do {
        value = bus->load(bus->context, 8);
        if ((value & 0x10) != 0 && (value & 0x40) == 0) {
            if (bus->load(bus->context, 0) == 0) return HR_STATUS_OK;
            HrRelease(bus);
            return HR_STATUS_BUSY;
        }
        stop = HrStop(bus);
        if (stop != HR_STATUS_OK || bus->now(bus->context) >= end) {
            HrRelease(bus);
            return stop == HR_STATUS_OK ? HR_STATUS_BUSY : stop;
        }
        bus->delay(bus->context);
    } while (1);
}

/* Fixed read-byte-data or receive-byte transaction. Caller cannot select these.
 * Host setup/status/semaphore writes are necessary to ISSUE A READ. No slave
 * data register is written, and every slave address has its read bit set. */
static uint32_t HrByte(HR_BUS *bus, uint8_t target, uint8_t command,
    int receive, uint8_t *byte)
{
    uint32_t result;
    uint8_t status, control;
    uint64_t end = bus->now(bus->context) + 20;
    int started = 0;
    if ((target < 0x50 || target > 0x57) && !(target == 0x36 && receive)) {
        return HR_STATUS_UNSUPPORTED;
    }
    result = HrStop(bus);
    if (result != HR_STATUS_OK) return result;
    control = bus->load(bus->context, 2);
    if ((control & 0x43) != 0) return HR_STATUS_BUSY;
    status = bus->load(bus->context, 0);
    if ((status & 0xbf) != 0) return HR_STATUS_BUSY;
    bus->store(bus->context, 4, (uint8_t)((target << 1) | 1));
    if (!receive) bus->store(bus->context, 3, command);
    bus->store(bus->context, 2, (uint8_t)(0x40 | (receive ? 0x04 : 0x08)));
    started = 1;
    do {
        status = bus->load(bus->context, 0);
        if ((status & 1) == 0 && (status & 0x1e) != 0) {
            if ((status & 0x10) != 0) result = HR_STATUS_IO_ERROR;
            else if ((status & 0x08) != 0) result = HR_STATUS_BUSY;
            else if ((status & 0x04) != 0) result = HR_STATUS_NOT_PRESENT;
            else { *byte = bus->load(bus->context, 5); result = HR_STATUS_OK; }
            bus->store(bus->context, 0, (uint8_t)(status & 0x1e));
            bus->store(bus->context, 2, control);
            return result;
        }
        result = HrStop(bus);
        if (result != HR_STATUS_OK || bus->now(bus->context) >= end) break;
        bus->delay(bus->context);
    } while (1);
    if (started) {
        uint64_t abort_end = bus->now(bus->context) + 20;
        /* Abort ONLY the read started under our semaphore. No bus reset/retry.
         * KILL is a host control operation, not an SMBus slave write. */
        bus->store(bus->context, 2, 0x02);
        do {
            status = bus->load(bus->context, 0);
            if ((status & 1) == 0) break;
            bus->delay(bus->context);
        } while (bus->now(bus->context) < abort_end);
        if ((status & 1) == 0) {
            bus->store(bus->context, 0, (uint8_t)(status & 0x1e));
            bus->store(bus->context, 2, control);
        }
        /* Even a successful abort disables further reads until removal/rebind. */
        bus->poisoned = 1;
    }
    return result == HR_STATUS_OK ? HR_STATUS_TIMEOUT : result;
}

static uint32_t HrPageZero(HR_BUS *bus)
{
    uint8_t page = 0xff;
    uint32_t status = HrByte(bus, 0x36, 0, 1, &page);
    /* EE1004 page selection requires a WRITE and is intentionally unsupported.
     * A receive-byte read of SPA0 must positively report page zero. */
    /* EE1004 SPA0 acknowledgement establishes page zero; data is unspecified. */
    return status == HR_STATUS_NOT_PRESENT ? HR_STATUS_UNSUPPORTED : status;
}

uint32_t HrSpdEnumerate(void *context, HR_INTERNAL_SLOT *slots, uint32_t *count)
{
    HR_BUS *bus = context;
    uint32_t status, page_status;
    uint8_t target, type = 0;
    *count = 0;
    bus->deadline = bus->now(bus->context) + 2000;
    status = HrClaim(bus);
    if (status != HR_STATUS_OK) return status;
    page_status = HrPageZero(bus);
    if (page_status != HR_STATUS_OK && page_status != HR_STATUS_UNSUPPORTED) {
        HrRelease(bus);
        return page_status;
    }
    /* A missing SPA0 responder is ambiguous: no EE1004 devices or DDR4 page
     * one selected. Do not interpret page-one bytes as a DDR3 type marker.
     * Legacy DDR3-only buses without a page responder remain unsupported. */
    if (page_status == HR_STATUS_UNSUPPORTED) {
        HrRelease(bus);
        return HR_STATUS_UNSUPPORTED;
    }
    /* Failed probes may not continue after timeout/cancellation. */
    if (bus->poisoned || HrStop(bus) != HR_STATUS_OK) {
        status = HrStop(bus);
        if (status == HR_STATUS_OK) status = HR_STATUS_TIMEOUT;
        HrRelease(bus);
        return status;
    }
    for (target = 0x50; target <= 0x57; ++target) {
        status = HrByte(bus, target, 2, 0, &type);
        if (status == HR_STATUS_NOT_PRESENT) continue;
        if (status != HR_STATUS_OK) { *count = 0; break; }
        if (type == 0x0b || (type == 0x0c && page_status == HR_STATUS_OK)) {
            slots[*count].target = target;
            slots[*count].generation = type == 0x0b ? HR_GENERATION_DDR3 : HR_GENERATION_DDR4;
            ++*count;
        }
        /* DDR5 hub NVM, other SPD generations and muxed buses remain unsupported. */
    }
    HrRelease(bus);
    return status == HR_STATUS_NOT_PRESENT ? HR_STATUS_OK : status;
}

uint32_t HrSpdRead(void *context, HR_INTERNAL_SLOT slot, uint8_t *bytes, uint32_t *count)
{
    HR_BUS *bus = context;
    uint32_t status, i;
    uint8_t type = 0;
    *count = 0;
    if (slot.target < 0x50 || slot.target > 0x57 ||
        (slot.generation != HR_GENERATION_DDR3 && slot.generation != HR_GENERATION_DDR4)) {
        return HR_STATUS_UNSUPPORTED;
    }
    bus->deadline = bus->now(bus->context) + 2000;
    status = HrClaim(bus);
    if (status != HR_STATUS_OK) return status;
    if (slot.generation == HR_GENERATION_DDR4) status = HrPageZero(bus);
    if (status == HR_STATUS_OK) status = HrByte(bus, slot.target, 2, 0, &type);
    if (status == HR_STATUS_OK && type != (slot.generation == HR_GENERATION_DDR3 ? 0x0b : 0x0c)) {
        status = HR_STATUS_NOT_PRESENT;
    }
    for (i = 0; status == HR_STATUS_OK && i < 256; ++i) {
        status = HrByte(bus, slot.target, (uint8_t)i, 0, bytes + i);
    }
    /* Check page again to reject firmware page switches during our capture. */
    if (status == HR_STATUS_OK && slot.generation == HR_GENERATION_DDR4) status = HrPageZero(bus);
    HrRelease(bus);
    if (status == HR_STATUS_OK) *count = 256;
    else memset(bytes, 0, HR_MAX_SPD_BYTES);
    return status;
}
