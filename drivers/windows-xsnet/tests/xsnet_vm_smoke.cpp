#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <cfgmgr32.h>
#include <io.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <vector>

namespace {

#pragma comment(lib, "Cfgmgr32.lib")
#pragma comment(lib, "Ws2_32.lib")

constexpr GUID kXsnetInterface = {
    0x09a04405,
    0x8a02,
    0x4398,
    {0xbb, 0xe0, 0x67, 0xed, 0x8a, 0x7b, 0x9e, 0x38},
};
constexpr DWORD kIoctlHello = 0x8337e000;
constexpr DWORD kIoctlAttach = 0x8337e004;
constexpr DWORD kIoctlSetLink = 0x8337e008;
constexpr DWORD kIoctlDequeueTx = 0x8337e00e;
constexpr DWORD kIoctlEnqueueRx = 0x8337e011;
constexpr DWORD kIoctlDetach = 0x8337e014;
constexpr DWORD kIoctlQueryIdentity = 0x8337e018;
constexpr std::uint32_t kMagic = 0x314e5358;
constexpr std::uint16_t kVersion = 1;
constexpr std::size_t kHeaderSize = 32;

void Write16(std::uint8_t* bytes, std::size_t offset, std::uint16_t value) {
    std::memcpy(bytes + offset, &value, sizeof(value));
}

void Write32(std::uint8_t* bytes, std::size_t offset, std::uint32_t value) {
    std::memcpy(bytes + offset, &value, sizeof(value));
}

void Write64(std::uint8_t* bytes, std::size_t offset, std::uint64_t value) {
    std::memcpy(bytes + offset, &value, sizeof(value));
}

std::uint64_t Read64(const std::uint8_t* bytes, std::size_t offset) {
    std::uint64_t value = 0;
    std::memcpy(&value, bytes + offset, sizeof(value));
    return value;
}

std::vector<std::uint8_t> Message(
    std::uint32_t type,
    std::uint64_t sequence,
    const std::uint8_t* payload,
    std::size_t payload_length) {
    std::vector<std::uint8_t> message(kHeaderSize + payload_length, 0);
    Write32(message.data(), 0, kMagic);
    Write16(message.data(), 4, kVersion);
    Write16(message.data(), 6, static_cast<std::uint16_t>(kHeaderSize));
    Write32(message.data(), 8, type);
    Write32(message.data(), 16, static_cast<std::uint32_t>(payload_length));
    Write64(message.data(), 24, sequence);
    if (payload_length != 0) {
        std::memcpy(message.data() + kHeaderSize, payload, payload_length);
    }
    return message;
}

bool Control(
    HANDLE device,
    DWORD ioctl,
    std::uint32_t type,
    std::uint64_t sequence,
    const std::uint8_t* payload,
    std::size_t payload_length) {
    auto message = Message(type, sequence, payload, payload_length);
    DWORD returned = 0;
    if (!DeviceIoControl(
            device,
            ioctl,
            message.data(),
            static_cast<DWORD>(message.size()),
            nullptr,
            0,
            &returned,
            nullptr)) {
        std::fprintf(
            stderr,
            "xsnet_vm_smoke control_failed ioctl=0x%08lx error=%lu\n",
            ioctl,
            GetLastError());
        return false;
    }
    if (returned != 0) {
        std::fprintf(stderr, "xsnet_vm_smoke unexpected_control_output=%lu\n", returned);
        return false;
    }
    return true;
}

HANDLE OpenDevice(std::uint64_t* interface_luid) {
    ULONG character_count = 0;
    if (CM_Get_Device_Interface_List_SizeW(
            &character_count,
            const_cast<GUID*>(&kXsnetInterface),
            nullptr,
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT) != CR_SUCCESS ||
        character_count < 2 || character_count > 32768) {
        std::fprintf(stderr, "xsnet_vm_smoke interface_size_failed\n");
        return INVALID_HANDLE_VALUE;
    }
    std::vector<wchar_t> paths(character_count, L'\0');
    if (CM_Get_Device_Interface_ListW(
            const_cast<GUID*>(&kXsnetInterface),
            nullptr,
            paths.data(),
            character_count,
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT) != CR_SUCCESS) {
        std::fprintf(stderr, "xsnet_vm_smoke interface_list_failed\n");
        return INVALID_HANDLE_VALUE;
    }
    const std::size_t path_length = std::wcslen(paths.data());
    if (path_length == 0 || path_length + 2 != character_count ||
        paths[path_length + 1] != L'\0') {
        std::fprintf(stderr, "xsnet_vm_smoke interface_list_not_unique\n");
        return INVALID_HANDLE_VALUE;
    }
    HANDLE device = CreateFileW(
        paths.data(),
        GENERIC_READ | GENERIC_WRITE,
        0,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (device == INVALID_HANDLE_VALUE) {
        std::fprintf(stderr, "xsnet_vm_smoke open_failed error=%lu\n", GetLastError());
        return INVALID_HANDLE_VALUE;
    }

    std::uint8_t request[8] = {1, 0, 8, 0, 0, 0, 0, 0};
    std::uint8_t response[16] = {};
    DWORD returned = 0;
    if (!DeviceIoControl(
            device,
            kIoctlQueryIdentity,
            request,
            sizeof(request),
            response,
            sizeof(response),
            &returned,
            nullptr) ||
        returned != sizeof(response) || response[0] != 1 || response[1] != 0 ||
        response[2] != 16 || response[3] != 0 ||
        std::memcmp(response + 4, "\0\0\0\0", 4) != 0 || Read64(response, 8) == 0) {
        std::fprintf(stderr, "xsnet_vm_smoke identity_failed error=%lu returned=%lu\n", GetLastError(), returned);
        CloseHandle(device);
        return INVALID_HANDLE_VALUE;
    }
    *interface_luid = Read64(response, 8);
    return device;
}

bool DrainTransmit(HANDLE device, std::uint64_t* sequence) {
    DWORD last_error = ERROR_SUCCESS;
    unsigned int dequeued_batches = 0;
    std::printf("xsnet_vm_smoke step=drain_transmit\n");
    for (unsigned int attempt = 0; attempt < 3000; ++attempt) {
        auto request = Message(4, *sequence, nullptr, 0);
        std::uint8_t response[4096] = {};
        DWORD returned = 0;
        if (attempt == 0) {
            std::printf("xsnet_vm_smoke step=dequeue_tx_first_call\n");
        }
        if (DeviceIoControl(
                device,
                kIoctlDequeueTx,
                request.data(),
                static_cast<DWORD>(request.size()),
                response,
                sizeof(response),
                &returned,
                nullptr)) {
            if (returned < kHeaderSize || Read64(response, 24) != *sequence) {
                std::fprintf(stderr, "xsnet_vm_smoke malformed_transmit_response=%lu\n", returned);
                return false;
            }
            ++dequeued_batches;
            ++*sequence;
            continue;
        }
        const DWORD error = GetLastError();
        last_error = error;
        if (error == ERROR_NOT_READY) {
            Sleep(10);
            continue;
        }
        std::printf(
            "xsnet_vm_smoke empty_transmit_error=%lu transmit_batches=%u\n",
            error,
            dequeued_batches);
        return error == ERROR_NO_MORE_ITEMS && dequeued_batches > 0;
    }
    std::fprintf(
        stderr,
        "xsnet_vm_smoke transmit_queue_never_drained last_error=%lu\n",
        last_error);
    return false;
}

bool QueueIpv4Transmit() {
    WSADATA winsock_data = {};
    if (WSAStartup(MAKEWORD(2, 2), &winsock_data) != 0) {
        std::fprintf(stderr, "xsnet_vm_smoke winsock_start_failed error=%d\n", WSAGetLastError());
        return false;
    }
    const SOCKET socket_handle = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (socket_handle == INVALID_SOCKET) {
        std::fprintf(stderr, "xsnet_vm_smoke socket_failed error=%d\n", WSAGetLastError());
        WSACleanup();
        return false;
    }
    sockaddr_in destination = {};
    destination.sin_family = AF_INET;
    destination.sin_port = htons(9);
    if (InetPtonA(AF_INET, "100.88.0.2", &destination.sin_addr) != 1) {
        std::fprintf(stderr, "xsnet_vm_smoke destination_parse_failed\n");
        closesocket(socket_handle);
        WSACleanup();
        return false;
    }
    const char payload[] = "xsnet-smoke";
    int sent = SOCKET_ERROR;
    int send_error = WSAEHOSTUNREACH;
    for (unsigned int attempt = 0; attempt < 200; ++attempt) {
        sent = sendto(
            socket_handle,
            payload,
            static_cast<int>(sizeof(payload) - 1),
            0,
            reinterpret_cast<const sockaddr*>(&destination),
            sizeof(destination));
        if (sent != SOCKET_ERROR) {
            break;
        }
        send_error = WSAGetLastError();
        if (send_error != WSAEHOSTUNREACH && send_error != WSAENETUNREACH) {
            break;
        }
        Sleep(25);
    }
    if (sent != static_cast<int>(sizeof(payload) - 1)) {
        std::fprintf(stderr, "xsnet_vm_smoke send_failed error=%d sent=%d\n", send_error, sent);
        closesocket(socket_handle);
        WSACleanup();
        return false;
    }
    closesocket(socket_handle);
    WSACleanup();
    std::printf("xsnet_vm_smoke step=queued_ipv4_transmit bytes=%d\n", sent);
    return true;
}

bool EnqueueReceive(HANDLE device, std::uint64_t sequence) {
    std::printf("xsnet_vm_smoke step=enqueue_receive\n");
    const std::uint8_t packet[20] = {
        0x45, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0xfd,
        0x00, 0x00, 100, 88, 0, 2, 100, 88, 0, 1,
    };
    std::uint8_t batch[36] = {};
    Write16(batch, 0, 1);
    Write32(batch, 4, 8);
    Write32(batch, 8, 16);
    Write32(batch, 12, sizeof(packet));
    std::memcpy(batch + 16, packet, sizeof(packet));
    auto message = Message(5, sequence, batch, sizeof(batch));
    DWORD returned = 0;
    if (!DeviceIoControl(
            device,
            kIoctlEnqueueRx,
            nullptr,
            0,
            message.data(),
            static_cast<DWORD>(message.size()),
            &returned,
            nullptr)) {
        std::fprintf(stderr, "xsnet_vm_smoke receive_failed error=%lu\n", GetLastError());
        return false;
    }
    return returned == 0;
}

bool RunSession(HANDLE device, bool exercise_queues) {
    std::printf("xsnet_vm_smoke step=hello\n");
    std::uint8_t hello[16] = {};
    Write16(hello, 0, 1);
    Write16(hello, 2, 1);
    Write32(hello, 4, 1);
    if (!Control(device, kIoctlHello, 1, 1, hello, sizeof(hello))) {
        return false;
    }
    std::uint8_t attach[16] = {};
    Write32(attach, 0, 1280);
    Write16(attach, 4, 4);
    Write16(attach, 6, 4);
    Write32(attach, 8, 1);
    if (!Control(device, kIoctlAttach, 2, 2, attach, sizeof(attach))) {
        return false;
    }
    std::printf("xsnet_vm_smoke step=link_up\n");
    std::uint8_t link[8] = {};
    Write32(link, 0, 1);
    if (!Control(device, kIoctlSetLink, 3, 3, link, sizeof(link))) {
        return false;
    }
    std::uint64_t sequence = 4;
    if (exercise_queues) {
        if (!QueueIpv4Transmit() || !DrainTransmit(device, &sequence) ||
            !EnqueueReceive(device, sequence)) {
            return false;
        }
        ++sequence;
    }
    std::memset(link, 0, sizeof(link));
    std::printf("xsnet_vm_smoke step=link_down\n");
    if (!Control(device, kIoctlSetLink, 3, sequence++, link, sizeof(link))) {
        return false;
    }
    return Control(device, kIoctlDetach, 6, sequence, nullptr, 0);
}

}  // namespace

int wmain(int argument_count, wchar_t** arguments) {
    if (argument_count == 2) {
        FILE* stream = nullptr;
        if (_wfreopen_s(&stream, arguments[1], L"w", stdout) != 0 ||
            _dup2(_fileno(stdout), _fileno(stderr)) != 0) {
            return 2;
        }
        setvbuf(stdout, nullptr, _IONBF, 0);
        setvbuf(stderr, nullptr, _IONBF, 0);
    } else if (argument_count != 1) {
        return 2;
    }

    std::uint64_t first_luid = 0;
    HANDLE device = OpenDevice(&first_luid);
    if (device == INVALID_HANDLE_VALUE) {
        return 1;
    }
    std::printf("xsnet_vm_smoke step=first_open interface_luid=%llu\n", first_luid);
    const bool first_pass = RunSession(device, true);
    CloseHandle(device);
    if (!first_pass) {
        return 1;
    }

    std::uint64_t second_luid = 0;
    device = OpenDevice(&second_luid);
    if (device == INVALID_HANDLE_VALUE) {
        return 1;
    }
    std::printf("xsnet_vm_smoke step=second_open interface_luid=%llu\n", second_luid);
    const bool second_pass = RunSession(device, false);
    CloseHandle(device);
    if (!second_pass || second_luid != first_luid) {
        std::fprintf(stderr, "xsnet_vm_smoke reopen_failed\n");
        return 1;
    }

    std::printf("xsnet_vm_smoke passed interface_luid=%llu\n", first_luid);
    return 0;
}
