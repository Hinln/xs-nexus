#include "xsnet_driver.h"

static XsnetMessageType message_type_from_ioctl(ULONG io_control_code) {
    switch (io_control_code) {
        case IOCTL_XSNET_HELLO:
            return XSNET_MESSAGE_HELLO;
        case IOCTL_XSNET_ATTACH:
            return XSNET_MESSAGE_ATTACH;
        case IOCTL_XSNET_SET_LINK:
            return XSNET_MESSAGE_SET_LINK;
        case IOCTL_XSNET_DEQUEUE_TX:
            return XSNET_MESSAGE_TX_BATCH;
        case IOCTL_XSNET_ENQUEUE_RX:
            return XSNET_MESSAGE_RX_BATCH;
        case IOCTL_XSNET_DETACH:
            return XSNET_MESSAGE_DETACH;
        default:
            return 0;
    }
}

static NTSTATUS status_from_session(XsnetSessionStatus session_status) {
    switch (session_status) {
        case XSNET_SESSION_OK:
            return STATUS_SUCCESS;
        case XSNET_SESSION_BAD_OWNER:
            return STATUS_ACCESS_DENIED;
        case XSNET_SESSION_BAD_STATE:
            return STATUS_INVALID_DEVICE_STATE;
        case XSNET_SESSION_BAD_VERSION_RANGE:
            return STATUS_REVISION_MISMATCH;
        case XSNET_SESSION_BAD_CAPABILITIES:
            return STATUS_NOT_SUPPORTED;
        case XSNET_SESSION_INVALID_ARGUMENT:
        case XSNET_SESSION_BAD_MESSAGE:
        case XSNET_SESSION_BAD_SEQUENCE:
        case XSNET_SESSION_BAD_MTU:
        case XSNET_SESSION_BAD_QUEUE_DEPTH:
        default:
            return STATUS_INVALID_PARAMETER;
    }
}

static NTSTATUS status_from_queue(XsnetQueueStatus queue_status) {
    switch (queue_status) {
        case XSNET_QUEUE_OK:
            return STATUS_SUCCESS;
        case XSNET_QUEUE_FULL:
            return STATUS_DEVICE_BUSY;
        case XSNET_QUEUE_EMPTY:
            return STATUS_NO_MORE_ENTRIES;
        case XSNET_QUEUE_BUFFER_TOO_SMALL:
            return STATUS_BUFFER_TOO_SMALL;
        case XSNET_QUEUE_BAD_PACKET:
        case XSNET_QUEUE_INVALID_ARGUMENT:
        default:
            return STATUS_INVALID_PARAMETER;
    }
}

static NTSTATUS validate_request_file(
    WDFREQUEST request,
    XsnetFileContext **file_context) {
    WDFFILEOBJECT file_object;

    if (WdfRequestGetRequestorMode(request) != UserMode) {
        return STATUS_ACCESS_DENIED;
    }
    file_object = WdfRequestGetFileObject(request);
    if (file_object == NULL) {
        return STATUS_ACCESS_DENIED;
    }
    *file_context = XsnetGetFileContext(file_object);
    if (!(*file_context)->accepted || (*file_context)->owner_cookie == 0) {
        return STATUS_ACCESS_DENIED;
    }
    return STATUS_SUCCESS;
}

static BOOLEAN transmit_queue_ready_locked(
    const XsnetDeviceContext *device_context) {
    XsnetQueueContext *queue_context;

    if (device_context->transmit_queue == NULL) {
        return FALSE;
    }
    queue_context = XsnetGetQueueContext(device_context->transmit_queue);
    return queue_context->started && !queue_context->cancelled;
}

static BOOLEAN receive_queue_ready_locked(
    const XsnetDeviceContext *device_context) {
    XsnetQueueContext *queue_context;

    if (device_context->receive_queue == NULL) {
        return FALSE;
    }
    queue_context = XsnetGetQueueContext(device_context->receive_queue);
    return queue_context->started && !queue_context->cancelled;
}

static void complete_transmit_request(
    WDFREQUEST request,
    XsnetDeviceContext *device_context,
    XsnetFileContext *file_context,
    size_t input_buffer_length,
    size_t output_buffer_length) {
    PVOID input_buffer;
    PVOID output_buffer;
    size_t actual_input_length;
    size_t actual_output_length;
    XsnetMessageView request_view;
    XsnetValidationStatus validation_status;
    XsnetSessionStatus session_status;
    XsnetSession previous_session;
    XsnetQueueStatus queue_status;
    uint32_t payload_length;
    uint16_t packet_count;
    uint32_t measured_payload_length;
    uint16_t measured_packet_count;
    NTSTATUS status;

    if (input_buffer_length != XSNET_ABI_HEADER_SIZE ||
        output_buffer_length > XSNET_ABI_HEADER_SIZE + XSNET_ABI_MAX_PAYLOAD) {
        WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE);
        return;
    }
    status = WdfRequestRetrieveInputBuffer(
        request,
        XSNET_ABI_HEADER_SIZE,
        &input_buffer,
        &actual_input_length);
    if (!NT_SUCCESS(status) || actual_input_length != input_buffer_length) {
        WdfRequestComplete(
            request,
            NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status);
        return;
    }
    status = WdfRequestRetrieveOutputBuffer(
        request,
        XSNET_ABI_HEADER_SIZE,
        &output_buffer,
        &actual_output_length);
    if (!NT_SUCCESS(status) || actual_output_length != output_buffer_length) {
        WdfRequestComplete(
            request,
            NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status);
        return;
    }
    validation_status = XsnetValidateMessage(
        input_buffer,
        actual_input_length,
        XSNET_MESSAGE_TX_BATCH,
        0,
        &request_view);
    if (validation_status != XSNET_VALID) {
        WdfRequestComplete(request, STATUS_INVALID_PARAMETER);
        return;
    }

    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    if (!file_context->accepted) {
        status = STATUS_ACCESS_DENIED;
        goto Exit;
    }
    if (!transmit_queue_ready_locked(device_context)) {
        status = STATUS_DEVICE_NOT_READY;
        goto Exit;
    }
    if (device_context->transmit_packets.count == 0) {
        status = STATUS_NO_MORE_ENTRIES;
        goto Exit;
    }
    queue_status = XsnetPacketQueueMeasureBatch(
        &device_context->transmit_packets,
        (uint32_t)(actual_output_length - XSNET_ABI_HEADER_SIZE),
        device_context->session.transmit_queue_depth,
        &measured_payload_length,
        &measured_packet_count);
    if (queue_status != XSNET_QUEUE_OK) {
        status = status_from_queue(queue_status);
        goto Exit;
    }
    validation_status = XsnetWriteMessageHeader(
        output_buffer,
        actual_output_length,
        XSNET_MESSAGE_TX_BATCH,
        measured_payload_length,
        request_view.sequence);
    if (validation_status != XSNET_VALID) {
        status = STATUS_INTERNAL_ERROR;
        goto Exit;
    }
    previous_session = device_context->session;
    session_status = XsnetSessionProcess(
        &device_context->session,
        file_context->owner_cookie,
        XSNET_MESSAGE_TX_BATCH,
        input_buffer,
        actual_input_length,
        &validation_status);
    if (session_status != XSNET_SESSION_OK) {
        status = status_from_session(session_status);
        goto Exit;
    }
    queue_status = XsnetPacketQueuePopBatch(
        &device_context->transmit_packets,
        (uint8_t *)output_buffer + XSNET_ABI_HEADER_SIZE,
        (uint32_t)(actual_output_length - XSNET_ABI_HEADER_SIZE),
        device_context->session.transmit_queue_depth,
        &payload_length,
        &packet_count);
    if (queue_status != XSNET_QUEUE_OK) {
        device_context->session = previous_session;
        status = status_from_queue(queue_status);
        goto Exit;
    }
    if (packet_count != measured_packet_count ||
        payload_length != measured_payload_length) {
        device_context->session = previous_session;
        status = STATUS_INTERNAL_ERROR;
        goto Exit;
    }
    WdfWaitLockRelease(device_context->session_lock);
    WdfRequestCompleteWithInformation(
        request,
        STATUS_SUCCESS,
        XSNET_ABI_HEADER_SIZE + payload_length);
    return;

Exit:
    WdfWaitLockRelease(device_context->session_lock);
    WdfRequestComplete(request, status);
}

static void complete_receive_request(
    WDFREQUEST request,
    XsnetDeviceContext *device_context,
    XsnetFileContext *file_context,
    size_t input_buffer_length,
    size_t output_buffer_length) {
    PVOID direct_buffer;
    size_t direct_buffer_length;
    XsnetMessageView message_view;
    XsnetValidationStatus validation_status;
    XsnetSessionStatus session_status;
    XsnetSession previous_session;
    XsnetQueueStatus queue_status;
    uint16_t packet_count;
    uint16_t accepted_packets;
    NETPACKETQUEUE receive_queue = NULL;
    BOOLEAN notify_receive_queue = FALSE;
    NTSTATUS status;

    if (input_buffer_length != 0 ||
        output_buffer_length < XSNET_ABI_HEADER_SIZE + 16 +
                                   XSNET_ABI_MIN_PACKET_SIZE ||
        output_buffer_length > XSNET_ABI_HEADER_SIZE + XSNET_ABI_MAX_PAYLOAD) {
        WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE);
        return;
    }
    status = WdfRequestRetrieveOutputBuffer(
        request,
        XSNET_ABI_HEADER_SIZE + 16 + XSNET_ABI_MIN_PACKET_SIZE,
        &direct_buffer,
        &direct_buffer_length);
    if (!NT_SUCCESS(status) || direct_buffer_length != output_buffer_length) {
        WdfRequestComplete(
            request,
            NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status);
        return;
    }
    validation_status = XsnetValidateMessage(
        direct_buffer,
        direct_buffer_length,
        XSNET_MESSAGE_RX_BATCH,
        XSNET_ABI_MAX_PAYLOAD,
        &message_view);
    if (validation_status != XSNET_VALID) {
        WdfRequestComplete(request, STATUS_INVALID_PARAMETER);
        return;
    }

    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    if (!file_context->accepted) {
        status = STATUS_ACCESS_DENIED;
        goto Exit;
    }
    if (!receive_queue_ready_locked(device_context)) {
        status = STATUS_DEVICE_NOT_READY;
        goto Exit;
    }
    queue_status = XsnetPacketQueueValidateBatch(
        &device_context->receive_packets,
        message_view.payload,
        message_view.payload_length,
        &packet_count);
    if (queue_status != XSNET_QUEUE_OK) {
        status = status_from_queue(queue_status);
        goto Exit;
    }
    if (packet_count == 0) {
        status = STATUS_INTERNAL_ERROR;
        goto Exit;
    }
    if (device_context->receive_packets.count >=
            device_context->session.receive_queue_depth ||
        packet_count >
            device_context->session.receive_queue_depth -
                device_context->receive_packets.count) {
        status = STATUS_DEVICE_BUSY;
        goto Exit;
    }
    previous_session = device_context->session;
    session_status = XsnetSessionProcess(
        &device_context->session,
        file_context->owner_cookie,
        XSNET_MESSAGE_RX_BATCH,
        direct_buffer,
        direct_buffer_length,
        &validation_status);
    if (session_status != XSNET_SESSION_OK) {
        status = status_from_session(session_status);
        goto Exit;
    }
    queue_status = XsnetPacketQueuePushBatch(
        &device_context->receive_packets,
        message_view.payload,
        message_view.payload_length,
        &accepted_packets);
    if (queue_status != XSNET_QUEUE_OK || accepted_packets != packet_count) {
        device_context->session = previous_session;
        status = STATUS_INTERNAL_ERROR;
        goto Exit;
    }
    receive_queue = device_context->receive_queue;
    WdfObjectReference(receive_queue);
    notify_receive_queue =
        XsnetGetQueueContext(receive_queue)->notification_enabled;
    status = STATUS_SUCCESS;

Exit:
    WdfWaitLockRelease(device_context->session_lock);
    if (receive_queue != NULL) {
        if (notify_receive_queue) {
            NetRxQueueNotifyMoreReceivedPacketsAvailable(receive_queue);
        }
        WdfObjectDereference(receive_queue);
    }
    WdfRequestComplete(request, status);
}

static void complete_control_request(
    WDFREQUEST request,
    WDFDEVICE device,
    XsnetDeviceContext *device_context,
    XsnetFileContext *file_context,
    XsnetMessageType message_type,
    size_t input_buffer_length) {
    PVOID input_buffer;
    size_t actual_input_length;
    XsnetValidationStatus validation_status;
    XsnetSessionStatus session_status;
    XsnetSession previous_session;
    BOOLEAN link_state_changed = FALSE;
    BOOLEAN link_connected = FALSE;
    BOOLEAN update_adapter_mtu = FALSE;
    uint32_t adapter_mtu = 0;
    NTSTATUS status;

    if (input_buffer_length < XSNET_ABI_HEADER_SIZE ||
        input_buffer_length > XSNET_ABI_HEADER_SIZE + XSNET_ABI_MAX_PAYLOAD) {
        WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE);
        return;
    }
    status = WdfRequestRetrieveInputBuffer(
        request,
        XSNET_ABI_HEADER_SIZE,
        &input_buffer,
        &actual_input_length);
    if (!NT_SUCCESS(status) || actual_input_length != input_buffer_length) {
        WdfRequestComplete(
            request,
            NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status);
        return;
    }
    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    previous_session = device_context->session;
    if (!file_context->accepted) {
        session_status = XSNET_SESSION_BAD_OWNER;
    } else {
        session_status = XsnetSessionProcess(
            &device_context->session,
            file_context->owner_cookie,
            message_type,
            input_buffer,
            actual_input_length,
            &validation_status);
    }
    status = status_from_session(session_status);
    if (session_status == XSNET_SESSION_OK &&
        message_type == XSNET_MESSAGE_ATTACH) {
        XsnetPacketQueueReset(&device_context->transmit_packets);
        XsnetPacketQueueReset(&device_context->receive_packets);
        device_context->transmit_packets.mtu = device_context->session.mtu;
        device_context->receive_packets.mtu = device_context->session.mtu;
        update_adapter_mtu = TRUE;
        adapter_mtu = device_context->session.mtu;
    } else if (session_status == XSNET_SESSION_OK &&
               message_type == XSNET_MESSAGE_SET_LINK) {
        if (device_context->session.state == XSNET_SESSION_LINK_UP &&
            (!transmit_queue_ready_locked(device_context) ||
             !receive_queue_ready_locked(device_context))) {
            device_context->session = previous_session;
            status = STATUS_DEVICE_NOT_READY;
        } else {
            link_state_changed = TRUE;
            link_connected =
                device_context->session.state == XSNET_SESSION_LINK_UP;
        }
    } else if (session_status == XSNET_SESSION_OK &&
               message_type == XSNET_MESSAGE_DETACH) {
        XsnetPacketQueueReset(&device_context->transmit_packets);
        XsnetPacketQueueReset(&device_context->receive_packets);
        device_context->transmit_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
        device_context->receive_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
        link_state_changed = TRUE;
        link_connected = FALSE;
    }
    WdfWaitLockRelease(device_context->session_lock);
    if (update_adapter_mtu) {
        NetAdapterSetLinkLayerMtuSize(
            device_context->adapter,
            adapter_mtu);
    }
    if (link_state_changed) {
        XsnetAdapterSetConnected(device, link_connected);
        if (link_connected) {
            status = WdfWaitLockAcquire(device_context->session_lock, NULL);
            if (NT_SUCCESS(status)) {
                link_connected =
                    device_context->session.state == XSNET_SESSION_LINK_UP &&
                    transmit_queue_ready_locked(device_context) &&
                    receive_queue_ready_locked(device_context);
                WdfWaitLockRelease(device_context->session_lock);
            } else {
                link_connected = FALSE;
            }
            if (!link_connected) {
                XsnetAdapterSetConnected(device, FALSE);
            }
        }
    }
    WdfRequestComplete(request, status);
}

void XsnetEvtIoDeviceControl(
    WDFQUEUE queue,
    WDFREQUEST request,
    size_t output_buffer_length,
    size_t input_buffer_length,
    ULONG io_control_code) {
    WDFDEVICE device = WdfIoQueueGetDevice(queue);
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    XsnetFileContext *file_context;
    XsnetMessageType message_type;
    NTSTATUS status;

    status = validate_request_file(request, &file_context);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    message_type = message_type_from_ioctl(io_control_code);
    if (message_type == 0) {
        WdfRequestComplete(request, STATUS_INVALID_DEVICE_REQUEST);
        return;
    }
    if (message_type == XSNET_MESSAGE_TX_BATCH) {
        complete_transmit_request(
            request,
            device_context,
            file_context,
            input_buffer_length,
            output_buffer_length);
        return;
    }
    if (message_type == XSNET_MESSAGE_RX_BATCH) {
        complete_receive_request(
            request,
            device_context,
            file_context,
            input_buffer_length,
            output_buffer_length);
        return;
    }
    if (output_buffer_length != 0) {
        WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE);
        return;
    }
    complete_control_request(
        request,
        device,
        device_context,
        file_context,
        message_type,
        input_buffer_length);
}

void XsnetEvtIoStop(
    WDFQUEUE queue,
    WDFREQUEST request,
    ULONG action_flags) {
    UNREFERENCED_PARAMETER(queue);
    UNREFERENCED_PARAMETER(action_flags);
    WdfRequestComplete(request, STATUS_CANCELLED);
}
