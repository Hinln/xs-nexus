#include <initguid.h>

#include "xsnet_driver.h"

DEFINE_GUID(
    GUID_DEVINTERFACE_XSNET,
    0x09a04405,
    0x8a02,
    0x4398,
    0xbb,
    0xe0,
    0x67,
    0xed,
    0x8a,
    0x7b,
    0x9e,
    0x38);

NTSTATUS DriverEntry(
    PDRIVER_OBJECT driver_object,
    PUNICODE_STRING registry_path) {
    WDF_DRIVER_CONFIG driver_config;
    WDF_OBJECT_ATTRIBUTES driver_attributes;

    WDF_DRIVER_CONFIG_INIT(&driver_config, XsnetEvtDeviceAdd);
    WDF_OBJECT_ATTRIBUTES_INIT(&driver_attributes);
    return WdfDriverCreate(
        driver_object,
        registry_path,
        &driver_attributes,
        &driver_config,
        WDF_NO_HANDLE);
}
