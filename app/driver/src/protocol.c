#include "protocol.h"
#include <string.h>

static void HrHeader(HR_HEADER *header, uint32_t size)
{
    header->version = HR_ABI_VERSION;
    header->size = size;
}

void HrSessionInit(HR_SESSION *session, uint32_t identity)
{
    memset(session, 0, sizeof(*session));
    session->identity = identity;
}

void HrSessionInvalidate(HR_SESSION *session)
{
    session->count = 0;
    session->epoch = 0;
    memset(session->tokens, 0, sizeof(session->tokens));
    memset(session->slots, 0, sizeof(session->slots));
}

int HrIoctlSizes(uint32_t ioctl, size_t *input, size_t *output)
{
    *input = sizeof(HR_HEADER);
    switch (ioctl) {
    case HR_IOCTL_CAPABILITIES: *output = sizeof(HR_CAPABILITIES); break;
    case HR_IOCTL_ENUMERATE_SPD: *output = sizeof(HR_ENUMERATE_REPLY); break;
    case HR_IOCTL_READ_SPD:
        *input = sizeof(HR_READ_REQUEST);
        *output = sizeof(HR_READ_REPLY);
        break;
    case HR_IOCTL_RESERVED_PPIN:
    case HR_IOCTL_RESERVED_DSN: *output = sizeof(HR_RESERVED_REPLY); break;
    default: *input = 0; *output = 0; return 0;
    }
    return 1;
}

HR_DISPATCH_RESULT HrDispatch(HR_SESSION *session, const HR_BACKEND *backend,
    uint32_t ioctl, const void *input, size_t input_size, void *output,
    size_t output_size, size_t *written)
{
    HR_READ_REQUEST request = {0};
    size_t required_input, required_output;
    uint32_t i, status;
    *written = 0;
    if (!HrIoctlSizes(ioctl, &required_input, &required_output)) {
        return HrDispatchUnknown;
    }
    if (input == NULL || input_size != required_input) return HrDispatchInvalid;
    if (output == NULL || output_size < required_output) return HrDispatchSmallOutput;
    /* Copy before zeroing: the kernel's input and output are the same allocation. */
    memcpy(&request, input, required_input);
    if (request.header.version != HR_ABI_VERSION ||
        request.header.size != required_input || request.header.reserved[0] != 0 ||
        request.header.reserved[1] != 0) return HrDispatchInvalid;
    memset(output, 0, required_output);
    HrHeader((HR_HEADER *)output, (uint32_t)required_output);
    *written = required_output;

    switch (ioctl) {
    case HR_IOCTL_CAPABILITIES: {
        HR_CAPABILITIES *reply = output;
        reply->status = HR_STATUS_OK;
        reply->max_slots = HR_MAX_SLOTS;
        reply->max_spd_bytes = HR_MAX_SPD_BYTES;
        reply->backend = backend->kind;
        reply->features = backend->kind == HR_BACKEND_NONE ? 0 :
            HR_FEATURE_SPD | HR_FEATURE_PARTIAL_SPD;
        break;
    }
    case HR_IOCTL_ENUMERATE_SPD: {
        HR_ENUMERATE_REPLY *reply = output;
        HrSessionInvalidate(session);
        if (backend->kind == HR_BACKEND_NONE || backend->enumerate == NULL) {
            reply->status = HR_STATUS_UNSUPPORTED;
            break;
        }
        status = backend->enumerate(backend->context, session->slots, &session->count);
        if (status != HR_STATUS_OK || session->count > HR_MAX_SLOTS ||
            session->identity == 0 || session->sequence > UINT32_MAX - HR_MAX_SLOTS) {
            HrSessionInvalidate(session);
            reply->status = status == HR_STATUS_OK ? HR_STATUS_IO_ERROR : status;
            break;
        }
        for (i = 0; i < session->count; ++i) {
            if (session->slots[i].target < 0x50 || session->slots[i].target > 0x57 ||
                (session->slots[i].generation != HR_GENERATION_DDR3 &&
                 session->slots[i].generation != HR_GENERATION_DDR4)) {
                HrSessionInvalidate(session);
                reply->status = HR_STATUS_IO_ERROR;
                return HrDispatchOk;
            }
        }
        session->epoch = backend->epoch;
        reply->count = session->count;
        for (i = 0; i < reply->count; ++i) {
            session->tokens[i] = ((uint64_t)session->identity << 32) | ++session->sequence;
            reply->slots[i].token = session->tokens[i];
            reply->slots[i].generation = session->slots[i].generation;
            reply->slots[i].controller = backend->kind;
            reply->slots[i].flags = session->slots[i].generation == HR_GENERATION_DDR4 ?
                HR_SPD_PARTIAL : 0;
        }
        break;
    }
    case HR_IOCTL_READ_SPD: {
        HR_READ_REPLY *reply = output;
        reply->token = request.token;
        if (session->epoch != backend->epoch || backend->kind == HR_BACKEND_NONE) {
            HrSessionInvalidate(session);
            reply->status = HR_STATUS_STALE_TOKEN;
            break;
        }
        for (i = 0; i < session->count && session->tokens[i] != request.token; ++i) {}
        if (request.token == 0 || i == session->count) {
            reply->status = HR_STATUS_STALE_TOKEN;
            break;
        }
        reply->status = backend->read == NULL ? HR_STATUS_UNSUPPORTED :
            backend->read(backend->context, session->slots[i], reply->bytes, &reply->byte_count);
        if (reply->status != HR_STATUS_OK || reply->byte_count != 256) {
            memset(reply->bytes, 0, sizeof(reply->bytes));
            reply->byte_count = 0;
            if (reply->status == HR_STATUS_OK) reply->status = HR_STATUS_IO_ERROR;
        } else {
            reply->generation = session->slots[i].generation;
            reply->flags = reply->generation == HR_GENERATION_DDR4 ? HR_SPD_PARTIAL : 0;
        }
        break;
    }
    default: ((HR_RESERVED_REPLY *)output)->status = HR_STATUS_UNSUPPORTED; break;
    }
    return HrDispatchOk;
}
