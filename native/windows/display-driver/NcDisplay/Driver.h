// Pilote d'écran indirect (IddCx) de NOTHING CONNECT.
// Dérivé de l'exemple IddSampleDriver de Microsoft (Windows-driver-samples,
// Microsoft Public License, voir LICENSE-MS-PL.txt dans ce dossier).
#pragma once

#define NOMINMAX
#include <windows.h>
#include <bugcodes.h>
#include <wudfwdm.h>
#include <wdf.h>
#include <iddcx.h>

#include <dxgi1_5.h>
#include <d3d11_2.h>
#include <avrt.h>
#include <wrl.h>

#include <memory>
#include <mutex>
#include <vector>

#include "..\shared\NcDisplayShared.h"

namespace Microsoft { namespace WRL { namespace Wrappers {
    typedef HandleT<HandleTraits::HANDLENullTraits> Thread;
} } }

namespace NothingConnect
{
    struct Direct3DDevice
    {
        explicit Direct3DDevice(LUID AdapterLuid) : AdapterLuid(AdapterLuid) {}
        HRESULT Init();

        LUID AdapterLuid;
        Microsoft::WRL::ComPtr<IDXGIFactory5> DxgiFactory;
        Microsoft::WRL::ComPtr<IDXGIAdapter1> Adapter;
        Microsoft::WRL::ComPtr<ID3D11Device> Device;
        Microsoft::WRL::ComPtr<ID3D11DeviceContext> DeviceContext;
    };

    /// Mémoire partagée vers l'application (une image BGRA + en-tête à verrou de séquence).
    class FrameSink
    {
    public:
        bool Open();
        void Close();
        void Write(const BYTE* pixels, UINT width, UINT height, UINT pitch);

    private:
        HANDLE m_hMap = nullptr;
        HANDLE m_hEvent = nullptr;
        BYTE* m_pView = nullptr;
    };

    class SwapChainProcessor
    {
    public:
        SwapChainProcessor(IDDCX_SWAPCHAIN hSwapChain, std::shared_ptr<Direct3DDevice> Device, HANDLE NewFrameEvent);
        ~SwapChainProcessor();

    private:
        static DWORD CALLBACK RunThread(LPVOID Argument);
        void Run();
        void RunCore();
        void CopyToSink(ID3D11Texture2D* pSurface);

        IDDCX_SWAPCHAIN m_hSwapChain;
        std::shared_ptr<Direct3DDevice> m_Device;
        HANDLE m_hAvailableBufferEvent;
        Microsoft::WRL::Wrappers::Thread m_hThread;
        Microsoft::WRL::Wrappers::Event m_hTerminateEvent;
        Microsoft::WRL::ComPtr<ID3D11Texture2D> m_Staging;
        D3D11_TEXTURE2D_DESC m_StagingDesc = {};
        FrameSink m_Sink;
    };

    class IndirectDeviceContext
    {
    public:
        explicit IndirectDeviceContext(_In_ WDFDEVICE WdfDevice);
        virtual ~IndirectDeviceContext();

        void InitAdapter();
        NTSTATUS AddMonitor(const NcAddMonitor& Request);
        NTSTATUS RemoveMonitor();

        IDDCX_ADAPTER Adapter() const { return m_Adapter; }

        // Mode demandé par l'application (taille de l'écran du téléphone).
        UINT Width = 1920, Height = 1080, RefreshHz = 60;

    protected:
        WDFDEVICE m_WdfDevice;
        IDDCX_ADAPTER m_Adapter;
        IDDCX_MONITOR m_Monitor = nullptr;
        std::mutex m_Lock;
    };

    class IndirectMonitorContext
    {
    public:
        explicit IndirectMonitorContext(_In_ IDDCX_MONITOR Monitor);
        virtual ~IndirectMonitorContext();

        void AssignSwapChain(IDDCX_SWAPCHAIN SwapChain, LUID RenderAdapter, HANDLE NewFrameEvent);
        void UnassignSwapChain();

    private:
        IDDCX_MONITOR m_Monitor;
        std::unique_ptr<SwapChainProcessor> m_ProcessingThread;
    };
}
