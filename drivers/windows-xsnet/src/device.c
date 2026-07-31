#include "xsnet_driver.h"

static NTSTATUS create_control_queue(WDFDEVICE device) {
    WDF_IO_QUEUE_CONFIG queue_config;
    WDF_OBJECT_ATTRIBUTES queue_attributes;
    WDFQUEUE queue;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(
        &queue_config,
        WdfIoQueueDispatchSequential);
    queue_config.EvtIoDeviceControl = XsnetEvtIoDeviceControl;
    queue_config.EvtIoStop = XsnetEvtIoStop;
    WDF_OBJECT_ATTRIBUTES_INIT(&queue_attributes);
    queue_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    queue_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    return WdfIoQueueCreate(
        device,
        &queue_config,
        &queue_attributes,
        &queue);
}

static NTSTATUS initialize_device_context(WDFDEVICE device) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    WDF_OBJECT_ATTRIBUTES lock_attributes;
    NTSTATUS status;

    WDF_OBJECT_ATTRIBUTES_INIT(&lock_attributes);
    lock_attributes.ParentObject = device;
    status = WdfWaitLockCreate(&lock_attributes, &device_context->session_lock);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    device_context->adapter = NULL;
    device_context->transmit_queue = NULL;
    device_context->receive_queue = NULL;
    device_context->next_owner_cookie = 0;
    device_context->adapter_started = FALSE;
    XsnetSessionInitialize(&device_context->session);
    if (XsnetPacketQueueInitialize(
            &device_context->transmit_packets,
            device_context->transmit_slots,
            XSNET_ABI_MAX_PACKET_SIZE) != XSNET_QUEUE_OK ||
        XsnetPacketQueueInitialize(
            &device_context->receive_packets,
            device_context->receive_slots,
            XSNET_ABI_MAX_PACKET_SIZE) != XSNET_QUEUE_OK) {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    return STATUS_SUCCESS;
}

NTSTATUS XsnetEvtDeviceAdd(
    WDFDRIVER driver,
    PWDFDEVICE_INIT device_init) {
    WDF_PNPPOWER_EVENT_CALLBACKS power_callbacks;
    WDF_FILEOBJECT_CONFIG file_config;
    WDF_OBJECT_ATTRIBUTES file_attributes;
    WDF_OBJECT_ATTRIBUTES device_attributes;
    WDFDEVICE device;
    XsnetDeviceContext *device_context;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(driver);

    status = NetDeviceInitConfig(device_init);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&power_callbacks);
    power_callbacks.EvtDevicePrepareHardware = XsnetEvtDevicePrepareHardware;
    power_callbacks.EvtDeviceReleaseHardware = XsnetEvtDeviceReleaseHardware;
    power_callbacks.EvtDeviceD0Entry = XsnetEvtDeviceD0Entry;
    power_callbacks.EvtDeviceD0Exit = XsnetEvtDeviceD0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(device_init, &power_callbacks);

    WDF_FILEOBJECT_CONFIG_INIT(
        &file_config,
        XsnetEvtDeviceFileCreate,
        XsnetEvtFileClose,
        XsnetEvtFileCleanup);
    file_config.FileObjectClass = WdfFileObjectWdfCannotUseFsContexts;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&file_attributes, XsnetFileContext);
    file_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    file_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    WdfDeviceInitSetFileObjectConfig(
        device_init,
        &file_config,
        &file_attributes);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&device_attributes, XsnetDeviceContext);
    device_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    device_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    status = WdfDeviceCreate(&device_init, &device_attributes, &device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = initialize_device_context(device);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = XsnetAdapterCreate(device, &XsnetGetDeviceContext(device)->adapter);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = WdfDeviceCreateDeviceInterface(
        device,
        &GUID_DEVINTERFACE_XSNET,
        NULL);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = create_control_queue(device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    device_context = XsnetGetDeviceContext(device);
    if (device_context->adapter == NULL) {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    return STATUS_SUCCESS;
}

void XsnetEvtDeviceFileCreate(
    WDFDEVICE device,
    WDFREQUEST request,
    WDFFILEOBJECT file_object) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    XsnetFileContext *file_context = XsnetGetFileContext(file_object);
    WDF_REQUEST_PARAMETERS request_parameters;
    PCUNICODE_STRING file_name;
    ACCESS_MASK desired_access;
    XsnetSessionStatus session_status;
    NTSTATUS status;

    file_context->owner_cookie = 0;
    file_context->requestor_process_id = 0;
    file_context->accepted = FALSE;
    if (WdfRequestGetRequestorMode(request) != UserMode) {
        WdfRequestComplete(request, STATUS_ACCESS_DENIED);
        return;
    }
    file_name = WdfFileObjectGetFileName(file_object);
    if (file_name == NULL || file_name->Length != 0) {
        WdfRequestComplete(request, STATUS_OBJECT_NAME_INVALID);
        return;
    }
    WDF_REQUEST_PARAMETERS_INIT(&request_parameters);
    WdfRequestGetParameters(request, &request_parameters);
    if (request_parameters.Parameters.Create.SecurityContext == NULL) {
        WdfRequestComplete(request, STATUS_ACCESS_DENIED);
        return;
    }
    desired_access = request_parameters.Parameters.Create.SecurityContext->DesiredAccess;
    if ((desired_access & (FILE_READ_DATA | FILE_WRITE_DATA)) !=
        (FILE_READ_DATA | FILE_WRITE_DATA) ||
        request_parameters.Parameters.Create.ShareAccess != 0) {
        WdfRequestComplete(request, STATUS_ACCESS_DENIED);
        return;
    }

    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    device_context->next_owner_cookie += 1;
    if (device_context->next_owner_cookie == 0) {
        device_context->next_owner_cookie += 1;
    }
    session_status = XsnetSessionOpen(
        &device_context->session,
        device_context->next_owner_cookie);
    if (session_status == XSNET_SESSION_OK) {
        file_context->owner_cookie = device_context->next_owner_cookie;
        file_context->requestor_process_id = WdfRequestGetRequestorProcessId(request);
        file_context->accepted = TRUE;
        XsnetPacketQueueReset(&device_context->transmit_packets);
        XsnetPacketQueueReset(&device_context->receive_packets);
    }
    WdfWaitLockRelease(device_context->session_lock);

    WdfRequestComplete(
        request,
        session_status == XSNET_SESSION_OK ? STATUS_SUCCESS : STATUS_SHARING_VIOLATION);
}

void XsnetEvtFileCleanup(WDFFILEOBJECT file_object) {
    WDFDEVICE device = WdfFileObjectGetDevice(file_object);
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    XsnetFileContext *file_context = XsnetGetFileContext(file_object);

    if (!file_context->accepted) {
        return;
    }
    if (NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        XsnetSessionClose(&device_context->session, file_context->owner_cookie);
        XsnetPacketQueueReset(&device_context->transmit_packets);
        XsnetPacketQueueReset(&device_context->receive_packets);
        file_context->accepted = FALSE;
        WdfWaitLockRelease(device_context->session_lock);
    }
    XsnetAdapterSetConnected(device, FALSE);
}

void XsnetEvtFileClose(WDFFILEOBJECT file_object) {
    XsnetFileContext *file_context = XsnetGetFileContext(file_object);

    file_context->owner_cookie = 0;
    file_context->requestor_process_id = 0;
    file_context->accepted = FALSE;
}

NTSTATUS XsnetEvtDevicePrepareHardware(
    WDFDEVICE device,
    WDFCMRESLIST resources_raw,
    WDFCMRESLIST resources_translated) {
    UNREFERENCED_PARAMETER(resources_raw);
    UNREFERENCED_PARAMETER(resources_translated);
    return XsnetAdapterStart(device);
}

NTSTATUS XsnetEvtDeviceReleaseHardware(
    WDFDEVICE device,
    WDFCMRESLIST resources_translated) {
    UNREFERENCED_PARAMETER(resources_translated);
    XsnetResetSession(device);
    XsnetAdapterStop(device);
    return STATUS_SUCCESS;
}

NTSTATUS XsnetEvtDeviceD0Entry(
    WDFDEVICE device,
    WDF_POWER_DEVICE_STATE previous_state) {
    UNREFERENCED_PARAMETER(previous_state);
    XsnetAdapterSetConnected(device, FALSE);
    return STATUS_SUCCESS;
}

NTSTATUS XsnetEvtDeviceD0Exit(
    WDFDEVICE device,
    WDF_POWER_DEVICE_STATE target_state) {
    UNREFERENCED_PARAMETER(target_state);
    XsnetResetSession(device);
    XsnetAdapterSetConnected(device, FALSE);
    return STATUS_SUCCESS;
}

void XsnetResetSession(WDFDEVICE device) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);

    if (NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        XsnetSessionInitialize(&device_context->session);
        XsnetPacketQueueReset(&device_context->transmit_packets);
        XsnetPacketQueueReset(&device_context->receive_packets);
        device_context->transmit_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
        device_context->receive_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
        WdfWaitLockRelease(device_context->session_lock);
    }
}
