#ifndef XSNET_DRIVER_H
#define XSNET_DRIVER_H

#include <windows.h>
#include <wdf.h>
#include <netadaptercx.h>

#include "xsnet_abi.h"
#include "xsnet_dataplane.h"
#include "xsnet_ioctl.h"
#include "xsnet_identity.h"
#include "xsnet_session.h"

#define XSNET_LINK_SPEED UINT64_C(1000000000)

typedef struct XsnetDeviceContext {
    WDFWAITLOCK session_lock;
    NETADAPTER adapter;
    XsnetSession session;
    XsnetPacketQueue transmit_packets;
    XsnetPacketQueue receive_packets;
    XsnetPacketSlot transmit_slots[XSNET_ABI_MAX_PACKETS];
    XsnetPacketSlot receive_slots[XSNET_ABI_MAX_PACKETS];
    NETPACKETQUEUE transmit_queue;
    NETPACKETQUEUE receive_queue;
    uint64_t next_owner_cookie;
    BOOLEAN adapter_started;
} XsnetDeviceContext;

typedef struct XsnetFileContext {
    uint64_t owner_cookie;
    ULONG requestor_process_id;
    BOOLEAN accepted;
} XsnetFileContext;

typedef enum XsnetQueueDirection {
    XSNET_QUEUE_TRANSMIT = 1,
    XSNET_QUEUE_RECEIVE = 2
} XsnetQueueDirection;

typedef struct XsnetQueueContext {
    XsnetQueueDirection direction;
    WDFDEVICE device;
    const NET_RING_COLLECTION *rings;
    NET_EXTENSION virtual_address_extension;
    BOOLEAN started;
    BOOLEAN notification_enabled;
    BOOLEAN cancelled;
} XsnetQueueContext;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(XsnetDeviceContext, XsnetGetDeviceContext);
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(XsnetFileContext, XsnetGetFileContext);
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(XsnetQueueContext, XsnetGetQueueContext);

extern const GUID GUID_DEVINTERFACE_XSNET;

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD XsnetEvtDeviceAdd;
EVT_WDF_DEVICE_FILE_CREATE XsnetEvtDeviceFileCreate;
EVT_WDF_FILE_CLEANUP XsnetEvtFileCleanup;
EVT_WDF_FILE_CLOSE XsnetEvtFileClose;
EVT_WDF_DEVICE_PREPARE_HARDWARE XsnetEvtDevicePrepareHardware;
EVT_WDF_DEVICE_RELEASE_HARDWARE XsnetEvtDeviceReleaseHardware;
EVT_WDF_DEVICE_D0_ENTRY XsnetEvtDeviceD0Entry;
EVT_WDF_DEVICE_D0_EXIT XsnetEvtDeviceD0Exit;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL XsnetEvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_STOP XsnetEvtIoStop;
EVT_NET_ADAPTER_CREATE_TXQUEUE XsnetEvtAdapterCreateTxQueue;
EVT_NET_ADAPTER_CREATE_RXQUEUE XsnetEvtAdapterCreateRxQueue;
EVT_PACKET_QUEUE_START XsnetEvtPacketQueueStart;
EVT_PACKET_QUEUE_STOP XsnetEvtPacketQueueStop;
EVT_PACKET_QUEUE_ADVANCE XsnetEvtPacketQueueAdvance;
EVT_PACKET_QUEUE_SET_NOTIFICATION_ENABLED XsnetEvtPacketQueueSetNotificationEnabled;
EVT_PACKET_QUEUE_CANCEL XsnetEvtPacketQueueCancel;

NTSTATUS XsnetAdapterCreate(
    WDFDEVICE device,
    NETADAPTER *adapter);

NTSTATUS XsnetAdapterStart(
    WDFDEVICE device);

void XsnetAdapterStop(
    WDFDEVICE device);

void XsnetAdapterSetConnected(
    WDFDEVICE device,
    BOOLEAN connected);

void XsnetResetSession(
    WDFDEVICE device);

#endif
