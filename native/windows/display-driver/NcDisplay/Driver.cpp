/*
    Pilote d'écran indirect (IddCx) de NOTHING CONNECT.

    Dérivé de l'exemple IddSampleDriver (Microsoft Windows-driver-samples, video/IndirectDisplay),
    distribué sous Microsoft Public License (voir LICENSE-MS-PL.txt). Modifications :
      * aucun écran n'est créé au démarrage : l'application le crée et le retire par IOCTL ;
      * un seul mode préféré, celui demandé par l'application (taille de l'écran du téléphone) ;
      * chaque image est copiée vers une mémoire partagée lue par l'application, qui l'encode
        et l'envoie au téléphone.
*/

#include "Driver.h"
#include <sddl.h>

using namespace std;
using namespace NothingConnect;
using namespace Microsoft::WRL;

static const GUID NC_DISPLAY_INTERFACE_GUID = NC_DISPLAY_INTERFACE_GUID_INIT;

// Un seul adaptateur virtuel par système.
static IndirectDeviceContext* g_Device = nullptr;

#pragma region helpers

static inline void FillSignalInfo(DISPLAYCONFIG_VIDEO_SIGNAL_INFO& Mode, DWORD Width, DWORD Height, DWORD VSync, bool bMonitorMode)
{
    Mode.totalSize.cx = Mode.activeSize.cx = Width;
    Mode.totalSize.cy = Mode.activeSize.cy = Height;
    Mode.AdditionalSignalInfo.vSyncFreqDivider = bMonitorMode ? 0 : 1;
    Mode.AdditionalSignalInfo.videoStandard = 255;
    Mode.vSyncFreq.Numerator = VSync;
    Mode.vSyncFreq.Denominator = 1;
    Mode.hSyncFreq.Numerator = VSync * Height;
    Mode.hSyncFreq.Denominator = 1;
    Mode.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    Mode.pixelRate = ((UINT64)VSync) * ((UINT64)Width) * ((UINT64)Height);
}

static IDDCX_MONITOR_MODE CreateIddCxMonitorMode(DWORD Width, DWORD Height, DWORD VSync)
{
    IDDCX_MONITOR_MODE Mode = {};
    Mode.Size = sizeof(Mode);
    Mode.Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER;
    FillSignalInfo(Mode.MonitorVideoSignalInfo, Width, Height, VSync, true);
    return Mode;
}

static IDDCX_TARGET_MODE CreateIddCxTargetMode(DWORD Width, DWORD Height, DWORD VSync)
{
    IDDCX_TARGET_MODE Mode = {};
    Mode.Size = sizeof(Mode);
    FillSignalInfo(Mode.TargetVideoSignalInfo.targetVideoSignalInfo, Width, Height, VSync, false);
    return Mode;
}

struct ModeSpec { DWORD W, H, Hz; };

/// Mode demandé (préféré) puis quelques modes de repli de même orientation.
static vector<ModeSpec> CurrentModes()
{
    vector<ModeSpec> candidates;
    UINT w = g_Device ? g_Device->Width : 1920, h = g_Device ? g_Device->Height : 1080, hz = g_Device ? g_Device->RefreshHz : 60;
    candidates.push_back({ w, h, hz });
    candidates.push_back({ w, h, 60 });
    candidates.push_back({ (w * 3 / 4) & ~1u, (h * 3 / 4) & ~1u, 60 }); // 75 %
    if (w >= h) { candidates.push_back({ 1280, 720, 60 }); candidates.push_back({ 1024, 768, 60 }); }
    else { candidates.push_back({ 720, 1280, 60 }); candidates.push_back({ 768, 1024, 60 }); }
    // Sans doublons : Windows refuse des listes de modes contenant deux fois le même mode.
    vector<ModeSpec> v;
    for (const auto& c : candidates)
    {
        bool dup = false;
        for (const auto& e : v) dup = dup || (e.W == c.W && e.H == c.H && e.Hz == c.Hz);
        if (!dup) v.push_back(c);
    }
    return v;
}

#pragma endregion

extern "C" DRIVER_INITIALIZE DriverEntry;

EVT_WDF_DRIVER_DEVICE_ADD NcDeviceAdd;
EVT_WDF_DEVICE_D0_ENTRY NcDeviceD0Entry;
EVT_IDD_CX_DEVICE_IO_CONTROL NcIoDeviceControl;
EVT_IDD_CX_ADAPTER_INIT_FINISHED NcAdapterInitFinished;
EVT_IDD_CX_ADAPTER_COMMIT_MODES NcAdapterCommitModes;
EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION NcParseMonitorDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES NcMonitorGetDefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES NcMonitorQueryModes;
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN NcMonitorAssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN NcMonitorUnassignSwapChain;

struct IndirectDeviceContextWrapper
{
    IndirectDeviceContext* pContext;
    void Cleanup() { delete pContext; pContext = nullptr; }
};

struct IndirectMonitorContextWrapper
{
    IndirectMonitorContext* pContext;
    void Cleanup() { delete pContext; pContext = nullptr; }
};

WDF_DECLARE_CONTEXT_TYPE(IndirectDeviceContextWrapper);
WDF_DECLARE_CONTEXT_TYPE(IndirectMonitorContextWrapper);

extern "C" BOOL WINAPI DllMain(_In_ HINSTANCE hInstance, _In_ UINT dwReason, _In_opt_ LPVOID lpReserved)
{
    UNREFERENCED_PARAMETER(hInstance);
    UNREFERENCED_PARAMETER(lpReserved);
    UNREFERENCED_PARAMETER(dwReason);
    return TRUE;
}

_Use_decl_annotations_
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT pDriverObject, PUNICODE_STRING pRegistryPath)
{
    WDF_DRIVER_CONFIG Config;
    WDF_OBJECT_ATTRIBUTES Attributes;
    WDF_OBJECT_ATTRIBUTES_INIT(&Attributes);
    WDF_DRIVER_CONFIG_INIT(&Config, NcDeviceAdd);
    return WdfDriverCreate(pDriverObject, pRegistryPath, &Attributes, &Config, WDF_NO_HANDLE);
}

_Use_decl_annotations_
NTSTATUS NcDeviceAdd(WDFDRIVER Driver, PWDFDEVICE_INIT pDeviceInit)
{
    UNREFERENCED_PARAMETER(Driver);

    WDF_PNPPOWER_EVENT_CALLBACKS PnpPowerCallbacks;
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&PnpPowerCallbacks);
    PnpPowerCallbacks.EvtDeviceD0Entry = NcDeviceD0Entry;
    WdfDeviceInitSetPnpPowerEventCallbacks(pDeviceInit, &PnpPowerCallbacks);

    IDD_CX_CLIENT_CONFIG IddConfig;
    IDD_CX_CLIENT_CONFIG_INIT(&IddConfig);
    // IddCx redirige les IOCTL vers une file interne : ce rappel est nécessaire pour les recevoir.
    IddConfig.EvtIddCxDeviceIoControl = NcIoDeviceControl;
    IddConfig.EvtIddCxAdapterInitFinished = NcAdapterInitFinished;
    IddConfig.EvtIddCxParseMonitorDescription = NcParseMonitorDescription;
    IddConfig.EvtIddCxMonitorGetDefaultDescriptionModes = NcMonitorGetDefaultModes;
    IddConfig.EvtIddCxMonitorQueryTargetModes = NcMonitorQueryModes;
    IddConfig.EvtIddCxAdapterCommitModes = NcAdapterCommitModes;
    IddConfig.EvtIddCxMonitorAssignSwapChain = NcMonitorAssignSwapChain;
    IddConfig.EvtIddCxMonitorUnassignSwapChain = NcMonitorUnassignSwapChain;

    NTSTATUS Status = IddCxDeviceInitConfig(pDeviceInit, &IddConfig);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectDeviceContextWrapper);
    Attr.EvtCleanupCallback = [](WDFOBJECT Object)
    {
        auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Object);
        if (pContext)
        {
            if (g_Device == pContext->pContext) g_Device = nullptr;
            pContext->Cleanup();
        }
    };

    WDFDEVICE Device = nullptr;
    Status = WdfDeviceCreate(&pDeviceInit, &Attr, &Device);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    Status = IddCxDeviceInitialize(Device);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    // Interface ouverte par l'application pour piloter l'écran virtuel.
    Status = WdfDeviceCreateDeviceInterface(Device, &NC_DISPLAY_INTERFACE_GUID, nullptr);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    pContext->pContext = new IndirectDeviceContext(Device);
    g_Device = pContext->pContext;
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcDeviceD0Entry(WDFDEVICE Device, WDF_POWER_DEVICE_STATE PreviousState)
{
    UNREFERENCED_PARAMETER(PreviousState);
    auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    pContext->pContext->InitAdapter();
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
VOID NcIoDeviceControl(WDFDEVICE Device, WDFREQUEST Request, size_t OutputBufferLength, size_t InputBufferLength, ULONG IoControlCode)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    auto* pWrapper = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    IndirectDeviceContext* pContext = pWrapper ? pWrapper->pContext : nullptr;
    NTSTATUS Status = STATUS_INVALID_DEVICE_REQUEST;
    ULONG_PTR Information = 0;

    if (pContext)
    {
        switch (IoControlCode)
        {
        case IOCTL_NC_ADD_MONITOR:
        {
            NcAddMonitor* pIn = nullptr;
            Status = WdfRequestRetrieveInputBuffer(Request, sizeof(NcAddMonitor), (PVOID*)&pIn, nullptr);
            if (NT_SUCCESS(Status))
            {
                Status = pContext->AddMonitor(*pIn);
            }
            break;
        }
        case IOCTL_NC_REMOVE_MONITOR:
            Status = pContext->RemoveMonitor();
            break;
        case IOCTL_NC_GET_VERSION:
        {
            UINT32* pOut = nullptr;
            Status = WdfRequestRetrieveOutputBuffer(Request, sizeof(UINT32), (PVOID*)&pOut, nullptr);
            if (NT_SUCCESS(Status))
            {
                *pOut = NC_DISPLAY_DRIVER_VERSION;
                Information = sizeof(UINT32);
            }
            break;
        }
        default:
            break;
        }
    }
    WdfRequestCompleteWithInformation(Request, Status, Information);
}

#pragma region Direct3DDevice

HRESULT Direct3DDevice::Init()
{
    HRESULT hr = CreateDXGIFactory2(0, IID_PPV_ARGS(&DxgiFactory));
    if (FAILED(hr)) return hr;
    hr = DxgiFactory->EnumAdapterByLuid(AdapterLuid, IID_PPV_ARGS(&Adapter));
    if (FAILED(hr)) return hr;
    return D3D11CreateDevice(Adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0,
        D3D11_SDK_VERSION, &Device, nullptr, &DeviceContext);
}

#pragma endregion

#pragma region FrameSink

bool FrameSink::Open()
{
    if (m_pView) return true;
    // Lecture seule pour l'utilisateur interactif ; accès complet système, service local, admins.
    PSECURITY_DESCRIPTOR pSdMap = nullptr;
    PSECURITY_DESCRIPTOR pSdEvent = nullptr;
    if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(L"D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)(A;;GR;;;IU)", SDDL_REVISION_1, &pSdMap, nullptr) ||
        !ConvertStringSecurityDescriptorToSecurityDescriptorW(L"D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)(A;;0x00100000;;;IU)", SDDL_REVISION_1, &pSdEvent, nullptr))
    {
        if (pSdMap) LocalFree(pSdMap);
        return false;
    }
    SECURITY_ATTRIBUTES saMap = { sizeof(saMap), pSdMap, FALSE };
    SECURITY_ATTRIBUTES saEvent = { sizeof(saEvent), pSdEvent, FALSE };
    m_hMap = CreateFileMappingW(INVALID_HANDLE_VALUE, &saMap, PAGE_READWRITE, 0, NC_SHARED_SIZE, NC_SHARED_NAME);
    m_hEvent = CreateEventW(&saEvent, FALSE, FALSE, NC_EVENT_NAME);
    LocalFree(pSdMap);
    LocalFree(pSdEvent);
    if (!m_hMap || !m_hEvent)
    {
        Close();
        return false;
    }
    m_pView = (BYTE*)MapViewOfFile(m_hMap, FILE_MAP_WRITE, 0, 0, NC_SHARED_SIZE);
    if (!m_pView)
    {
        Close();
        return false;
    }
    auto* hdr = (NcFrameHeader*)m_pView;
    hdr->magic = NC_MAGIC;
    hdr->version = NC_DISPLAY_DRIVER_VERSION;
    return true;
}

void FrameSink::Close()
{
    if (m_pView) { UnmapViewOfFile(m_pView); m_pView = nullptr; }
    if (m_hMap) { CloseHandle(m_hMap); m_hMap = nullptr; }
    if (m_hEvent) { CloseHandle(m_hEvent); m_hEvent = nullptr; }
}

void FrameSink::Write(const BYTE* pixels, UINT width, UINT height, UINT pitch)
{
    if (!m_pView || width > NC_MAX_WIDTH || height > NC_MAX_HEIGHT) return;
    auto* hdr = (NcFrameHeader*)m_pView;
    InterlockedIncrement64((volatile LONG64*)&hdr->seq); // impair : écriture en cours
    hdr->width = width;
    hdr->height = height;
    hdr->stride = width * 4;
    hdr->format = 87; // DXGI_FORMAT_B8G8R8A8_UNORM
    BYTE* dst = m_pView + NC_HEADER_SIZE;
    for (UINT y = 0; y < height; y++)
    {
        memcpy(dst + (SIZE_T)y * width * 4, pixels + (SIZE_T)y * pitch, (SIZE_T)width * 4);
    }
    MemoryBarrier();
    InterlockedIncrement64((volatile LONG64*)&hdr->seq); // pair : image complète
    SetEvent(m_hEvent);
}

#pragma endregion

#pragma region SwapChainProcessor

SwapChainProcessor::SwapChainProcessor(IDDCX_SWAPCHAIN hSwapChain, shared_ptr<Direct3DDevice> Device, HANDLE NewFrameEvent)
    : m_hSwapChain(hSwapChain), m_Device(Device), m_hAvailableBufferEvent(NewFrameEvent)
{
    m_hTerminateEvent.Attach(CreateEvent(nullptr, FALSE, FALSE, nullptr));
    m_hThread.Attach(CreateThread(nullptr, 0, RunThread, this, 0, nullptr));
}

SwapChainProcessor::~SwapChainProcessor()
{
    SetEvent(m_hTerminateEvent.Get());
    if (m_hThread.Get())
    {
        WaitForSingleObject(m_hThread.Get(), INFINITE);
    }
    m_Sink.Close();
}

DWORD CALLBACK SwapChainProcessor::RunThread(LPVOID Argument)
{
    reinterpret_cast<SwapChainProcessor*>(Argument)->Run();
    return 0;
}

void SwapChainProcessor::Run()
{
    DWORD AvTask = 0;
    HANDLE AvTaskHandle = AvSetMmThreadCharacteristicsW(L"Distribution", &AvTask);
    m_Sink.Open();
    RunCore();
    // Supprimer la chaîne d'échange relance une nouvelle chaîne côté système si nécessaire.
    WdfObjectDelete((WDFOBJECT)m_hSwapChain);
    m_hSwapChain = nullptr;
    AvRevertMmThreadCharacteristics(AvTaskHandle);
}

void SwapChainProcessor::CopyToSink(ID3D11Texture2D* pSurface)
{
    D3D11_TEXTURE2D_DESC desc;
    pSurface->GetDesc(&desc);
    if (!m_Staging || m_StagingDesc.Width != desc.Width || m_StagingDesc.Height != desc.Height || m_StagingDesc.Format != desc.Format)
    {
        m_Staging.Reset();
        D3D11_TEXTURE2D_DESC s = desc;
        s.Usage = D3D11_USAGE_STAGING;
        s.BindFlags = 0;
        s.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        s.MiscFlags = 0;
        s.MipLevels = 1;
        s.ArraySize = 1;
        s.SampleDesc.Count = 1;
        s.SampleDesc.Quality = 0;
        if (FAILED(m_Device->Device->CreateTexture2D(&s, nullptr, &m_Staging))) return;
        m_StagingDesc = s;
    }
    m_Device->DeviceContext->CopyResource(m_Staging.Get(), pSurface);
    D3D11_MAPPED_SUBRESOURCE mapped;
    if (SUCCEEDED(m_Device->DeviceContext->Map(m_Staging.Get(), 0, D3D11_MAP_READ, 0, &mapped)))
    {
        if (desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM || desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM_SRGB)
        {
            m_Sink.Write((const BYTE*)mapped.pData, desc.Width, desc.Height, mapped.RowPitch);
        }
        m_Device->DeviceContext->Unmap(m_Staging.Get(), 0);
    }
}

void SwapChainProcessor::RunCore()
{
    ComPtr<IDXGIDevice> DxgiDevice;
    HRESULT hr = m_Device->Device.As(&DxgiDevice);
    if (FAILED(hr)) return;

    IDARG_IN_SWAPCHAINSETDEVICE SetDevice = {};
    SetDevice.pDevice = DxgiDevice.Get();
    hr = IddCxSwapChainSetDevice(m_hSwapChain, &SetDevice);
    if (FAILED(hr)) return;

    for (;;)
    {
        ComPtr<IDXGIResource> AcquiredBuffer;
        IDARG_OUT_RELEASEANDACQUIREBUFFER Buffer = {};
        hr = IddCxSwapChainReleaseAndAcquireBuffer(m_hSwapChain, &Buffer);

        if (hr == E_PENDING)
        {
            HANDLE WaitHandles[] = { m_hAvailableBufferEvent, m_hTerminateEvent.Get() };
            DWORD WaitResult = WaitForMultipleObjects(ARRAYSIZE(WaitHandles), WaitHandles, FALSE, 16);
            if (WaitResult == WAIT_OBJECT_0 || WaitResult == WAIT_TIMEOUT) continue;
            break;
        }
        else if (SUCCEEDED(hr))
        {
            AcquiredBuffer.Attach(Buffer.MetaData.pSurface);
            ComPtr<ID3D11Texture2D> Texture;
            if (SUCCEEDED(AcquiredBuffer.As(&Texture)))
            {
                CopyToSink(Texture.Get());
            }
            AcquiredBuffer.Reset();
            hr = IddCxSwapChainFinishedProcessingFrame(m_hSwapChain);
            if (FAILED(hr)) break;
        }
        else
        {
            break; // chaîne abandonnée (ex. DXGI_ERROR_ACCESS_LOST)
        }
    }
}

#pragma endregion

#pragma region IndirectDeviceContext

IndirectDeviceContext::IndirectDeviceContext(_In_ WDFDEVICE WdfDevice) : m_WdfDevice(WdfDevice)
{
    m_Adapter = {};
}

IndirectDeviceContext::~IndirectDeviceContext()
{
}

void IndirectDeviceContext::InitAdapter()
{
    IDDCX_ADAPTER_CAPS AdapterCaps = {};
    AdapterCaps.Size = sizeof(AdapterCaps);
    AdapterCaps.MaxMonitorsSupported = 1;
    AdapterCaps.EndPointDiagnostics.Size = sizeof(AdapterCaps.EndPointDiagnostics);
    AdapterCaps.EndPointDiagnostics.GammaSupport = IDDCX_FEATURE_IMPLEMENTATION_NONE;
    AdapterCaps.EndPointDiagnostics.TransmissionType = IDDCX_TRANSMISSION_TYPE_WIRELESS_OTHER;
    AdapterCaps.EndPointDiagnostics.pEndPointFriendlyName = L"NOTHING CONNECT";
    AdapterCaps.EndPointDiagnostics.pEndPointManufacturerName = L"NOTHING CONNECT";
    AdapterCaps.EndPointDiagnostics.pEndPointModelName = L"Phone display";

    IDDCX_ENDPOINT_VERSION Version = {};
    Version.Size = sizeof(Version);
    Version.MajorVer = NC_DISPLAY_DRIVER_VERSION;
    AdapterCaps.EndPointDiagnostics.pFirmwareVersion = &Version;
    AdapterCaps.EndPointDiagnostics.pHardwareVersion = &Version;

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectDeviceContextWrapper);

    IDARG_IN_ADAPTER_INIT AdapterInit = {};
    AdapterInit.WdfDevice = m_WdfDevice;
    AdapterInit.pCaps = &AdapterCaps;
    AdapterInit.ObjectAttributes = &Attr;

    IDARG_OUT_ADAPTER_INIT AdapterInitOut;
    NTSTATUS Status = IddCxAdapterInitAsync(&AdapterInit, &AdapterInitOut);
    if (NT_SUCCESS(Status))
    {
        m_Adapter = AdapterInitOut.AdapterObject;
        auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(AdapterInitOut.AdapterObject);
        pContext->pContext = this;
    }
}

NTSTATUS IndirectDeviceContext::AddMonitor(const NcAddMonitor& Request)
{
    if (Request.width < 320 || Request.height < 320 || Request.width > NC_MAX_WIDTH || Request.height > NC_MAX_HEIGHT)
    {
        return STATUS_INVALID_PARAMETER;
    }
    lock_guard<mutex> guard(m_Lock);
    if (!m_Adapter) return STATUS_DEVICE_NOT_READY;
    if (m_Monitor)
    {
        // Changement de résolution : on débranche puis rebranche l'écran.
        IddCxMonitorDeparture(m_Monitor);
        m_Monitor = nullptr;
    }
    Width = Request.width & ~1u;
    Height = Request.height & ~1u;
    RefreshHz = (Request.refresh_hz >= 24 && Request.refresh_hz <= 120) ? Request.refresh_hz : 60;

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectMonitorContextWrapper);
    Attr.EvtCleanupCallback = [](WDFOBJECT Object)
    {
        auto* pContext = WdfObjectGet_IndirectMonitorContextWrapper(Object);
        if (pContext) pContext->Cleanup();
    };

    IDDCX_MONITOR_INFO MonitorInfo = {};
    MonitorInfo.Size = sizeof(MonitorInfo);
    MonitorInfo.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRELESS;
    MonitorInfo.ConnectorIndex = 0;
    MonitorInfo.MonitorDescription.Size = sizeof(MonitorInfo.MonitorDescription);
    MonitorInfo.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    MonitorInfo.MonitorDescription.DataSize = 0; // sans EDID : modes fournis par le pilote
    MonitorInfo.MonitorDescription.pData = nullptr;
    // Identifiant de conteneur stable : Windows mémorise la position de l'écran d'une fois sur l'autre.
    MonitorInfo.MonitorContainerId = { 0x9b0e3c71, 0x55a2, 0x4c1f, { 0x8d, 0x3e, 0x61, 0x2a, 0x7f, 0x04, 0xb9, 0xc5 } };

    IDARG_IN_MONITORCREATE MonitorCreate = {};
    MonitorCreate.ObjectAttributes = &Attr;
    MonitorCreate.pMonitorInfo = &MonitorInfo;

    IDARG_OUT_MONITORCREATE MonitorCreateOut;
    NTSTATUS Status = IddCxMonitorCreate(m_Adapter, &MonitorCreate, &MonitorCreateOut);
    if (!NT_SUCCESS(Status)) return Status;

    auto* pMonitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorCreateOut.MonitorObject);
    pMonitorContextWrapper->pContext = new IndirectMonitorContext(MonitorCreateOut.MonitorObject);

    IDARG_OUT_MONITORARRIVAL ArrivalOut;
    Status = IddCxMonitorArrival(MonitorCreateOut.MonitorObject, &ArrivalOut);
    if (NT_SUCCESS(Status))
    {
        m_Monitor = MonitorCreateOut.MonitorObject;
    }
    return Status;
}

NTSTATUS IndirectDeviceContext::RemoveMonitor()
{
    lock_guard<mutex> guard(m_Lock);
    if (!m_Monitor) return STATUS_SUCCESS;
    NTSTATUS Status = IddCxMonitorDeparture(m_Monitor);
    m_Monitor = nullptr;
    return Status;
}

IndirectMonitorContext::IndirectMonitorContext(_In_ IDDCX_MONITOR Monitor) : m_Monitor(Monitor)
{
}

IndirectMonitorContext::~IndirectMonitorContext()
{
    m_ProcessingThread.reset();
}

void IndirectMonitorContext::AssignSwapChain(IDDCX_SWAPCHAIN SwapChain, LUID RenderAdapter, HANDLE NewFrameEvent)
{
    m_ProcessingThread.reset();
    auto Device = make_shared<Direct3DDevice>(RenderAdapter);
    if (FAILED(Device->Init()))
    {
        WdfObjectDelete(SwapChain);
    }
    else
    {
        m_ProcessingThread.reset(new SwapChainProcessor(SwapChain, Device, NewFrameEvent));
    }
}

void IndirectMonitorContext::UnassignSwapChain()
{
    m_ProcessingThread.reset();
}

#pragma endregion

#pragma region DDI Callbacks

_Use_decl_annotations_
NTSTATUS NcAdapterInitFinished(IDDCX_ADAPTER AdapterObject, const IDARG_IN_ADAPTER_INIT_FINISHED* pInArgs)
{
    // Aucun écran n'est branché tant que l'application ne le demande pas.
    UNREFERENCED_PARAMETER(AdapterObject);
    UNREFERENCED_PARAMETER(pInArgs);
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcAdapterCommitModes(IDDCX_ADAPTER AdapterObject, const IDARG_IN_COMMITMODES* pInArgs)
{
    UNREFERENCED_PARAMETER(AdapterObject);
    UNREFERENCED_PARAMETER(pInArgs);
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcParseMonitorDescription(const IDARG_IN_PARSEMONITORDESCRIPTION* pInArgs, IDARG_OUT_PARSEMONITORDESCRIPTION* pOutArgs)
{
    // Les écrans sont créés sans EDID : cet appel ne devrait pas avoir lieu.
    auto modes = CurrentModes();
    pOutArgs->MonitorModeBufferOutputCount = (UINT)modes.size();
    if (pInArgs->MonitorModeBufferInputCount < modes.size())
    {
        return (pInArgs->MonitorModeBufferInputCount > 0) ? STATUS_BUFFER_TOO_SMALL : STATUS_SUCCESS;
    }
    for (size_t i = 0; i < modes.size(); i++)
    {
        pInArgs->pMonitorModes[i] = CreateIddCxMonitorMode(modes[i].W, modes[i].H, modes[i].Hz);
    }
    pOutArgs->PreferredMonitorModeIdx = 0;
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcMonitorGetDefaultModes(IDDCX_MONITOR MonitorObject, const IDARG_IN_GETDEFAULTDESCRIPTIONMODES* pInArgs, IDARG_OUT_GETDEFAULTDESCRIPTIONMODES* pOutArgs)
{
    UNREFERENCED_PARAMETER(MonitorObject);
    auto modes = CurrentModes();
    pOutArgs->DefaultMonitorModeBufferOutputCount = (UINT)modes.size();
    if (pInArgs->DefaultMonitorModeBufferInputCount == 0)
    {
        return STATUS_SUCCESS;
    }
    if (pInArgs->DefaultMonitorModeBufferInputCount < modes.size())
    {
        return STATUS_BUFFER_TOO_SMALL;
    }
    for (size_t i = 0; i < modes.size(); i++)
    {
        pInArgs->pDefaultMonitorModes[i] = CreateIddCxMonitorMode(modes[i].W, modes[i].H, modes[i].Hz);
    }
    pOutArgs->PreferredMonitorModeIdx = 0;
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcMonitorQueryModes(IDDCX_MONITOR MonitorObject, const IDARG_IN_QUERYTARGETMODES* pInArgs, IDARG_OUT_QUERYTARGETMODES* pOutArgs)
{
    UNREFERENCED_PARAMETER(MonitorObject);
    vector<IDDCX_TARGET_MODE> TargetModes;
    for (const auto& m : CurrentModes())
    {
        TargetModes.push_back(CreateIddCxTargetMode(m.W, m.H, m.Hz));
    }
    pOutArgs->TargetModeBufferOutputCount = (UINT)TargetModes.size();
    if (pInArgs->TargetModeBufferInputCount >= TargetModes.size())
    {
        copy(TargetModes.begin(), TargetModes.end(), pInArgs->pTargetModes);
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcMonitorAssignSwapChain(IDDCX_MONITOR MonitorObject, const IDARG_IN_SETSWAPCHAIN* pInArgs)
{
    auto* pMonitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorObject);
    pMonitorContextWrapper->pContext->AssignSwapChain(pInArgs->hSwapChain, pInArgs->RenderAdapterLuid, pInArgs->hNextSurfaceAvailable);
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS NcMonitorUnassignSwapChain(IDDCX_MONITOR MonitorObject)
{
    auto* pMonitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorObject);
    pMonitorContextWrapper->pContext->UnassignSwapChain();
    return STATUS_SUCCESS;
}

#pragma endregion
