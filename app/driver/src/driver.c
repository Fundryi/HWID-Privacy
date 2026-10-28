#include <ntddk.h>
#include <wdf.h>
#include <initguid.h>
#include <wdmguid.h>
#include "smbus.h"

/* One root shell plus, optionally, one explicitly bound PCI controller FDO.
 * No machine-wide PCI scan, physical-memory primitive, filter or global callback. */
typedef struct HR_DRIVER_CONTEXT {
    WDFWAITLOCK lock;
    WDFDEVICE control;
    WDFQUEUE queue;
    WDFDEVICE controller;
    ULONG devices;
    ULONG64 next_session;
    ULONG64 epoch;
} HR_DRIVER_CONTEXT;
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(HR_DRIVER_CONTEXT, HrDriverContext)

typedef struct HR_DEVICE_CONTEXT {
    BUS_INTERFACE_STANDARD pci;
    BOOLEAN pci_valid;
    BOOLEAN counted;
    BOOLEAN ready;
    volatile LONG stopping;
    USHORT base;
    PUCHAR pm;
    ULONG pm_length;
    uint32_t kind;
    HR_BUS bus;
    WDFREQUEST request;
    WDFFILEOBJECT file;
} HR_DEVICE_CONTEXT;
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(HR_DEVICE_CONTEXT, HrDeviceContext)

typedef struct HR_FILE_CONTEXT {
    volatile LONG closing;
    HR_SESSION session;
} HR_FILE_CONTEXT;
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(HR_FILE_CONTEXT, HrFileContext)

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD HrDeviceAdd;
EVT_WDF_DRIVER_UNLOAD HrUnload;
EVT_WDF_DEVICE_PREPARE_HARDWARE HrPrepareHardware;
EVT_WDF_DEVICE_RELEASE_HARDWARE HrReleaseHardware;
EVT_WDF_DEVICE_D0_ENTRY HrD0Entry;
EVT_WDF_DEVICE_D0_EXIT HrD0Exit;
EVT_WDF_DEVICE_SURPRISE_REMOVAL HrSurpriseRemoval;
EVT_WDF_OBJECT_CONTEXT_CLEANUP HrDeviceCleanup;
EVT_WDF_DEVICE_FILE_CREATE HrFileCreate;
EVT_WDF_FILE_CLEANUP HrFileCleanup;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL HrIoctl;

static uint8_t HrPortRead(void *context, uint8_t offset)
{
    HR_DEVICE_CONTEXT *device = context;
    NT_ASSERT(offset < 0x20 && device->base != 0);
    /* Driver-owned translated I/O resource. No user value contributes to a pointer. */
    return READ_PORT_UCHAR((PUCHAR)(ULONG_PTR)((ULONG)device->base + offset));
}

static void HrPortWrite(void *context, uint8_t offset, uint8_t value)
{
    HR_DEVICE_CONTEXT *device = context;
    NT_ASSERT(offset < 0x20 && device->base != 0);
    WRITE_PORT_UCHAR((PUCHAR)(ULONG_PTR)((ULONG)device->base + offset), value);
}

static uint64_t HrNow(void *context)
{
    UNREFERENCED_PARAMETER(context);
    return KeQueryInterruptTime() / 10000;
}

static void HrDelay(void *context)
{
    LARGE_INTEGER interval;
    UNREFERENCED_PARAMETER(context);
    interval.QuadPart = -10000; /* PASSIVE_LEVEL polling, no busy-loop or worker thread. */
    (void)KeDelayExecutionThread(KernelMode, FALSE, &interval);
}

static int HrCancelled(void *context)
{
    HR_DEVICE_CONTEXT *device = context;
    return InterlockedCompareExchange(&device->stopping, 0, 0) != 0 ||
        device->file == NULL ||
        InterlockedCompareExchange(&HrFileContext(device->file)->closing, 0, 0) != 0 ||
        device->request == NULL || WdfRequestIsCanceled(device->request);
}

static HR_DRIVER_CONTEXT *HrGlobal(WDFDEVICE device)
{
    return HrDriverContext(WdfDeviceGetDriver(device));
}

static NTSTATUS HrCreateControl(WDFDRIVER driver)
{
    HR_DRIVER_CONTEXT *global = HrDriverContext(driver);
    DECLARE_CONST_UNICODE_STRING(sddl, L"D:P(A;;GA;;;SY)(A;;GA;;;BA)");
    DECLARE_CONST_UNICODE_STRING(name, L"\\Device\\HWIDReader");
    DECLARE_CONST_UNICODE_STRING(link, L"\\DosDevices\\HWIDReader");
    PWDFDEVICE_INIT init = WdfControlDeviceInitAllocate(driver, &sddl);
    WDF_OBJECT_ATTRIBUTES attributes, file_attributes;
    WDF_FILEOBJECT_CONFIG files;
    WDF_IO_QUEUE_CONFIG queue;
    NTSTATUS status;
    if (init == NULL) return STATUS_INSUFFICIENT_RESOURCES;
    WdfDeviceInitSetDeviceType(init, HR_DEVICE_TYPE);
    WdfDeviceInitSetCharacteristics(init, FILE_DEVICE_SECURE_OPEN, TRUE);
    WdfDeviceInitSetExclusive(init, TRUE);
    WdfDeviceInitSetIoType(init, WdfDeviceIoBuffered);
    status = WdfDeviceInitAssignName(init, &name);
    if (!NT_SUCCESS(status)) { WdfDeviceInitFree(init); return status; }
    WDF_FILEOBJECT_CONFIG_INIT(&files, HrFileCreate, WDF_NO_EVENT_CALLBACK, HrFileCleanup);
    files.FileObjectClass = WdfFileObjectWdfCanUseFsContext;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&file_attributes, HR_FILE_CONTEXT);
    file_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    file_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    WdfDeviceInitSetFileObjectConfig(init, &files, &file_attributes);
    WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
    attributes.ExecutionLevel = WdfExecutionLevelPassive;
    attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    status = WdfDeviceCreate(&init, &attributes, &global->control);
    if (!NT_SUCCESS(status)) { if (init != NULL) WdfDeviceInitFree(init); return status; }
    status = WdfDeviceCreateSymbolicLink(global->control, &link);
    if (NT_SUCCESS(status)) {
        WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queue, WdfIoQueueDispatchSequential);
        queue.PowerManaged = WdfFalse; /* Control devices have no PnP power state. */
        queue.EvtIoDeviceControl = HrIoctl;
        status = WdfIoQueueCreate(global->control, &queue, WDF_NO_OBJECT_ATTRIBUTES, &global->queue);
    }
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(global->control);
        global->control = NULL;
        global->queue = NULL;
        return status;
    }
    WdfControlFinishInitializing(global->control);
    return STATUS_SUCCESS;
}

NTSTATUS DriverEntry(PDRIVER_OBJECT object, PUNICODE_STRING registry)
{
    WDF_DRIVER_CONFIG config;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDFDRIVER driver;
    HR_DRIVER_CONTEXT *global;
    NTSTATUS status;
    ExInitializeDriverRuntime(DrvRtPoolNxOptIn);
    WDF_DRIVER_CONFIG_INIT(&config, HrDeviceAdd);
    config.EvtDriverUnload = HrUnload;
    config.DriverPoolTag = 'rIwH';
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, HR_DRIVER_CONTEXT);
    attributes.ExecutionLevel = WdfExecutionLevelPassive;
    status = WdfDriverCreate(object, registry, &attributes, &config, &driver);
    if (!NT_SUCCESS(status)) return status;
    global = HrDriverContext(driver);
    global->epoch = 1;
    WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
    attributes.ParentObject = driver;
    return WdfWaitLockCreate(&attributes, &global->lock);
}

NTSTATUS HrDeviceAdd(WDFDRIVER driver, PWDFDEVICE_INIT init)
{
    WDF_PNPPOWER_EVENT_CALLBACKS callbacks;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDFDEVICE device;
    HR_DRIVER_CONTEXT *global = HrDriverContext(driver);
    NTSTATUS status;
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&callbacks);
    callbacks.EvtDevicePrepareHardware = HrPrepareHardware;
    callbacks.EvtDeviceReleaseHardware = HrReleaseHardware;
    callbacks.EvtDeviceD0Entry = HrD0Entry;
    callbacks.EvtDeviceD0Exit = HrD0Exit;
    callbacks.EvtDeviceSurpriseRemoval = HrSurpriseRemoval;
    WdfDeviceInitSetPnpPowerEventCallbacks(init, &callbacks);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, HR_DEVICE_CONTEXT);
    attributes.ExecutionLevel = WdfExecutionLevelPassive;
    attributes.EvtCleanupCallback = HrDeviceCleanup;
    status = WdfDeviceCreate(&init, &attributes, &device);
    if (!NT_SUCCESS(status)) return status;
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (global->control == NULL) status = HrCreateControl(driver);
    if (NT_SUCCESS(status)) {
        ++global->devices;
        HrDeviceContext(device)->counted = TRUE;
    }
    WdfWaitLockRelease(global->lock);
    return status;
}

/* Returns only a BIOS-assigned, PnP-owned I/O port resource matching the
 * driver's verified base. Mapping an arbitrary physical address is absent. */
static BOOLEAN HrOwnsPort(WDFCMRESLIST raw, WDFCMRESLIST translated, ULONG base)
{
    ULONG i;
    if (base == 0 || base > 0xffe0 || WdfCmResourceListGetCount(raw) !=
        WdfCmResourceListGetCount(translated)) return FALSE;
    for (i = 0; i < WdfCmResourceListGetCount(translated); ++i) {
        PCM_PARTIAL_RESOURCE_DESCRIPTOR r = WdfCmResourceListGetDescriptor(raw, i);
        PCM_PARTIAL_RESOURCE_DESCRIPTOR t = WdfCmResourceListGetDescriptor(translated, i);
        if (r != NULL && t != NULL && r->Type == CmResourceTypePort &&
            t->Type == CmResourceTypePort && (t->Flags & CM_RESOURCE_PORT_IO) != 0 &&
            r->u.Port.Start.QuadPart == base && t->u.Port.Start.QuadPart == base &&
            r->u.Port.Length >= 0x20 && t->u.Port.Length >= 0x20 &&
            r->ShareDisposition == CmResourceShareDeviceExclusive &&
            t->ShareDisposition == CmResourceShareDeviceExclusive) return TRUE;
    }
    return FALSE;
}

NTSTATUS HrPrepareHardware(WDFDEVICE device, WDFCMRESLIST raw, WDFCMRESLIST translated)
{
    HR_DEVICE_CONTEXT *context = HrDeviceContext(device);
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    uint8_t config[256] = {0};
    ULONG i, base = 0;
    NTSTATUS status;
    context->stopping = 1;
    status = WdfFdoQueryForInterface(device, &GUID_BUS_INTERFACE_STANDARD,
        (PINTERFACE)&context->pci, sizeof(context->pci), 1, NULL);
    /* The hardware-agnostic root shell owns no PCI interface and reads nothing. */
    if (!NT_SUCCESS(status)) return STATUS_SUCCESS;
    context->pci_valid = TRUE;
    if (context->pci.GetBusData == NULL || context->pci.InterfaceDereference == NULL ||
        context->pci.GetBusData(context->pci.Context, PCI_WHICHSPACE_CONFIG,
            config, 0, sizeof(config)) != sizeof(config)) return STATUS_SUCCESS;
    context->kind = HrPciFamily(config);
    if (context->kind == HR_BACKEND_INTEL_I801) {
        base = HrIntelBase(config);
    } else if (context->kind == HR_BACKEND_AMD_FCH) {
        /* Modern AMD base is NOT a PCI BAR. It comes from FCH PM DecodeEn.
         * Map PM only if Windows assigned the documented range to this FDO.
         * TODO(hardware): prove the owned ACPI/PnP PM resource route per board;
         * most PCI SMBus devnodes do not own it and must remain unsupported. */
        for (i = 0; i < WdfCmResourceListGetCount(translated); ++i) {
            PCM_PARTIAL_RESOURCE_DESCRIPTOR r = WdfCmResourceListGetDescriptor(translated, i);
            if (r != NULL && r->Type == CmResourceTypeMemory &&
                r->u.Memory.Start.QuadPart == 0xfed80300 && r->u.Memory.Length >= 8 &&
                r->ShareDisposition == CmResourceShareDeviceExclusive) {
                context->pm_length = 8;
                context->pm = MmMapIoSpaceEx(r->u.Memory.Start, 8, PAGE_READONLY | PAGE_NOCACHE);
                if (context->pm != NULL) base = HrAmdBase(READ_REGISTER_ULONG((PULONG)context->pm));
                break;
            }
        }
    }
    if (!HrOwnsPort(raw, translated, base)) {
        context->kind = HR_BACKEND_NONE;
        return STATUS_SUCCESS;
    }
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (global->controller != NULL && global->controller != device) {
        context->kind = HR_BACKEND_NONE;
    } else {
        global->controller = device;
        context->base = (USHORT)base;
        context->bus.context = context;
        context->bus.kind = context->kind;
        context->bus.load = HrPortRead;
        context->bus.store = HrPortWrite;
        context->bus.now = HrNow;
        context->bus.delay = HrDelay;
        context->bus.cancelled = HrCancelled;
    }
    WdfWaitLockRelease(global->lock);
    return STATUS_SUCCESS;
}

NTSTATUS HrD0Entry(WDFDEVICE device, WDF_POWER_DEVICE_STATE previous)
{
    HR_DEVICE_CONTEXT *context = HrDeviceContext(device);
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    UNREFERENCED_PARAMETER(previous);
    (void)WdfWaitLockAcquire(global->lock, NULL);
    /* Never clear a timed-out controller's poison on resume; removal/rebind is required. */
    context->ready = context->base != 0 && !context->bus.poisoned;
    InterlockedExchange(&context->stopping, 0);
    ++global->epoch;
    WdfWaitLockRelease(global->lock);
    return STATUS_SUCCESS;
}

static void HrStopDevice(WDFDEVICE device)
{
    HR_DEVICE_CONTEXT *context = HrDeviceContext(device);
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    InterlockedExchange(&context->stopping, 1);
    /* Active reads see stopping between polls. This lock is their rundown. */
    (void)WdfWaitLockAcquire(global->lock, NULL);
    context->ready = FALSE;
    ++global->epoch;
    WdfWaitLockRelease(global->lock);
}

NTSTATUS HrD0Exit(WDFDEVICE device, WDF_POWER_DEVICE_STATE target)
{
    UNREFERENCED_PARAMETER(target);
    HrStopDevice(device);
    return STATUS_SUCCESS;
}

void HrSurpriseRemoval(WDFDEVICE device)
{
    HrStopDevice(device);
}

NTSTATUS HrReleaseHardware(WDFDEVICE device, WDFCMRESLIST translated)
{
    HR_DEVICE_CONTEXT *context = HrDeviceContext(device);
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    UNREFERENCED_PARAMETER(translated);
    HrStopDevice(device);
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (global->controller == device) global->controller = NULL;
    context->base = 0;
    context->kind = HR_BACKEND_NONE;
    if (context->pm != NULL) {
        MmUnmapIoSpace(context->pm, context->pm_length);
        context->pm = NULL;
    }
    if (context->pci_valid) {
        if (context->pci.InterfaceDereference != NULL)
            context->pci.InterfaceDereference(context->pci.Context);
        context->pci_valid = FALSE;
    }
    WdfWaitLockRelease(global->lock);
    return STATUS_SUCCESS;
}

void HrDeviceCleanup(WDFOBJECT object)
{
    WDFDEVICE device = (WDFDEVICE)object;
    HR_DEVICE_CONTEXT *context = HrDeviceContext(device);
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    WDFDEVICE control = NULL;
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (context->counted && --global->devices == 0) {
        control = global->control;
        global->control = NULL;
        global->queue = NULL;
    }
    WdfWaitLockRelease(global->lock);
    /* PnP+control KMDF drivers must delete control after the last FDO's cleanup
     * to allow unload. Existing file/IRP references finish before deletion. */
    if (control != NULL) WdfObjectDelete(control);
}

void HrUnload(WDFDRIVER driver)
{
    HR_DRIVER_CONTEXT *global = HrDriverContext(driver);
    NT_ASSERT(global->devices == 0 && global->controller == NULL && global->control == NULL);
    /* WDF owns remaining lock/context lifetime. No callbacks, timers, workers,
     * allocations, device mappings or PCI interface references remain. */
}

void HrFileCreate(WDFDEVICE device, WDFREQUEST request, WDFFILEOBJECT file)
{
    HR_DRIVER_CONTEXT *global = HrGlobal(device);
    NTSTATUS status = STATUS_SUCCESS;
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (++global->next_session > UINT32_MAX) status = STATUS_INTEGER_OVERFLOW;
    else {
        HrSessionInit(&HrFileContext(file)->session, (uint32_t)global->next_session);
        if (global->queue != NULL) WdfIoQueueStart(global->queue);
    }
    WdfWaitLockRelease(global->lock);
    WdfRequestComplete(request, status);
}

void HrFileCleanup(WDFFILEOBJECT file)
{
    HR_DRIVER_CONTEXT *global = HrGlobal(WdfFileObjectGetDevice(file));
    WDFQUEUE queue;
    InterlockedExchange(&HrFileContext(file)->closing, 1);
    (void)WdfWaitLockAcquire(global->lock, NULL);
    queue = global->queue;
    if (queue != NULL) WdfObjectReference(queue);
    WdfWaitLockRelease(global->lock);
    /* Exclusive open: purging this queue cannot cancel another client's I/O. */
    if (queue != NULL) {
        WdfIoQueuePurgeSynchronously(queue);
        WdfObjectDereference(queue);
    }
    (void)WdfWaitLockAcquire(global->lock, NULL);
    HrSessionInvalidate(&HrFileContext(file)->session);
    WdfWaitLockRelease(global->lock);
}

void HrIoctl(WDFQUEUE queue, WDFREQUEST request, size_t output_size,
    size_t input_size, ULONG ioctl)
{
    WDFDEVICE control = WdfIoQueueGetDevice(queue);
    HR_DRIVER_CONTEXT *global = HrGlobal(control);
    HR_DEVICE_CONTEXT *context = NULL;
    WDFFILEOBJECT file = WdfRequestGetFileObject(request);
    HR_BACKEND backend = {0};
    HR_DISPATCH_RESULT result;
    PVOID input, output;
    size_t required_input, required_output, written = 0;
    NTSTATUS status;
    if (!HrIoctlSizes(ioctl, &required_input, &required_output)) {
        WdfRequestComplete(request, STATUS_INVALID_DEVICE_REQUEST);
        return;
    }
    if (file == NULL || input_size != required_input) {
        WdfRequestComplete(request, STATUS_INVALID_PARAMETER);
        return;
    }
    if (output_size < required_output) {
        WdfRequestComplete(request, STATUS_BUFFER_TOO_SMALL);
        return;
    }
    status = WdfRequestRetrieveInputBuffer(request, required_input, &input, NULL);
    if (NT_SUCCESS(status)) status = WdfRequestRetrieveOutputBuffer(request, required_output, &output, NULL);
    if (!NT_SUCCESS(status)) { WdfRequestComplete(request, status); return; }
    (void)WdfWaitLockAcquire(global->lock, NULL);
    if (InterlockedCompareExchange(&HrFileContext(file)->closing, 0, 0) != 0 ||
        WdfRequestIsCanceled(request)) {
        WdfWaitLockRelease(global->lock);
        WdfRequestComplete(request, STATUS_CANCELLED);
        return;
    }
    backend.epoch = global->epoch;
    if (global->controller != NULL) {
        context = HrDeviceContext(global->controller);
        if (context->ready) {
            context->request = request;
            context->file = file;
            backend.context = &context->bus;
            backend.kind = context->kind;
            backend.enumerate = HrSpdEnumerate;
            backend.read = HrSpdRead;
        }
    }
    result = HrDispatch(&HrFileContext(file)->session, &backend, ioctl,
        input, input_size, output, output_size, &written);
    if (context != NULL) { context->request = NULL; context->file = NULL; }
    WdfWaitLockRelease(global->lock);
    status = result == HrDispatchOk ? STATUS_SUCCESS : STATUS_INVALID_PARAMETER;
    if (WdfRequestIsCanceled(request) || InterlockedCompareExchange(&HrFileContext(file)->closing, 0, 0) != 0) {
        status = STATUS_CANCELLED;
        written = 0;
    }
    WdfRequestCompleteWithInformation(request, status, NT_SUCCESS(status) ? written : 0);
}
