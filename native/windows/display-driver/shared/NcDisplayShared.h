// Interface partagée entre le pilote NcDisplay, l'outil d'installation et l'application
// NOTHING CONNECT (dupliquée côté Rust dans apps/windows/src-tauri/src/display.rs).
#pragma once
#include <stdint.h>

// {6F7C2A1E-4B3D-4E8A-9C51-2D7B8E0F4A63}
#define NC_DISPLAY_INTERFACE_GUID_INIT { 0x6f7c2a1e, 0x4b3d, 0x4e8a, { 0x9c, 0x51, 0x2d, 0x7b, 0x8e, 0x0f, 0x4a, 0x63 } }

#define NC_DISPLAY_HWID L"NothingConnectDisplay"

// Codes IOCTL : FILE_DEVICE_UNKNOWN (0x22), METHOD_BUFFERED, FILE_ANY_ACCESS.
#define NC_IOCTL(fn) (((0x22) << 16) | ((0) << 14) | ((fn) << 2) | (0))
#define IOCTL_NC_ADD_MONITOR    NC_IOCTL(0x800)
#define IOCTL_NC_REMOVE_MONITOR NC_IOCTL(0x801)
#define IOCTL_NC_GET_VERSION    NC_IOCTL(0x802)

#define NC_DISPLAY_DRIVER_VERSION 1u

#pragma pack(push, 1)
struct NcAddMonitor {
    uint32_t width;
    uint32_t height;
    uint32_t refresh_hz;
};
#pragma pack(pop)

// Mémoire partagée (lecture seule pour l'utilisateur interactif).
#define NC_SHARED_NAME L"Global\\NothingConnectDisplayFrame"
#define NC_EVENT_NAME  L"Global\\NothingConnectDisplayEvent"
#define NC_MAX_WIDTH   2560u
#define NC_MAX_HEIGHT  2560u
#define NC_MAGIC       0x4644434Eu /* 'NCDF' */

#pragma pack(push, 1)
struct NcFrameHeader {
    uint32_t magic;
    uint32_t version;
    // Verrou de séquence : impair pendant l'écriture, pair quand l'image est complète.
    volatile int64_t seq;
    uint32_t width;
    uint32_t height;
    uint32_t stride;
    uint32_t format; // 87 = DXGI_FORMAT_B8G8R8A8_UNORM
    uint32_t reserved[8];
};
#pragma pack(pop)

#define NC_HEADER_SIZE 64u
#define NC_SHARED_SIZE (NC_HEADER_SIZE + NC_MAX_WIDTH * NC_MAX_HEIGHT * 4u)
static_assert(sizeof(NcFrameHeader) == NC_HEADER_SIZE, "en-tête de 64 octets");
