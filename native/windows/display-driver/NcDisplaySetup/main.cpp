// NcDisplaySetup : installe / désinstalle le pilote d'écran NOTHING CONNECT.
//
//   NcDisplaySetup status                 code 0 installé, 2 absent
//   NcDisplaySetup install <dossier>      installe le pilote + crée le périphérique (élévation)
//   NcDisplaySetup uninstall              retire périphérique, pilote et certificat (élévation)
//   NcDisplaySetup uninstall-if-present   n'élève que si le pilote est présent (désinstalleur)
//   NcDisplaySetup enable-testsigning     active la signature de test (élévation, redémarrage)
//
// Codes de sortie : 0 OK, 1 erreur, 2 absent, 3 annulé par l'utilisateur (UAC),
// 10 signature de test désactivée, 11 bloqué par le démarrage sécurisé, 12 redémarrage requis.

#define NOMINMAX
#include <windows.h>
#include <shellapi.h>
#include <setupapi.h>
#include <newdev.h>
#include <swdevice.h>
#include <cfgmgr32.h>
#include <wincrypt.h>
#include <cstdio>
#include <string>
#include <vector>

#include "..\shared\NcDisplayShared.h"

static const GUID NC_GUID = NC_DISPLAY_INTERFACE_GUID_INIT;

static bool IsElevated()
{
    HANDLE token = nullptr;
    TOKEN_ELEVATION elev = {};
    DWORD len = 0;
    bool ok = OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token) &&
              GetTokenInformation(token, TokenElevation, &elev, sizeof(elev), &len) && elev.TokenIsElevated;
    if (token) CloseHandle(token);
    return ok;
}

/// Relance ce programme en administrateur avec les mêmes arguments et renvoie son code.
static int RelaunchElevated(const std::wstring& args)
{
    wchar_t self[MAX_PATH];
    GetModuleFileNameW(nullptr, self, MAX_PATH);
    SHELLEXECUTEINFOW sei = { sizeof(sei) };
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.lpVerb = L"runas";
    sei.lpFile = self;
    sei.lpParameters = args.c_str();
    sei.nShow = SW_HIDE;
    if (!ShellExecuteExW(&sei))
    {
        return GetLastError() == ERROR_CANCELLED ? 3 : 1;
    }
    WaitForSingleObject(sei.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(sei.hProcess, &code);
    CloseHandle(sei.hProcess);
    return (int)code;
}

static bool InterfacePresent()
{
    HDEVINFO h = SetupDiGetClassDevsW(&NC_GUID, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (h == INVALID_HANDLE_VALUE) return false;
    SP_DEVICE_INTERFACE_DATA d = { sizeof(d) };
    bool found = SetupDiEnumDeviceInterfaces(h, nullptr, &NC_GUID, 0, &d) != FALSE;
    SetupDiDestroyDeviceInfoList(h);
    return found;
}

static bool TestSigningEnabled()
{
    HKEY k;
    if (RegOpenKeyExW(HKEY_LOCAL_MACHINE, L"SYSTEM\\CurrentControlSet\\Control", 0, KEY_READ, &k) != ERROR_SUCCESS) return false;
    wchar_t buf[1024] = {};
    DWORD size = sizeof(buf);
    bool on = RegQueryValueExW(k, L"SystemStartOptions", nullptr, nullptr, (LPBYTE)buf, &size) == ERROR_SUCCESS &&
              wcsstr(_wcsupr(buf), L"TESTSIGNING") != nullptr;
    RegCloseKey(k);
    return on;
}

static bool AddCertificate(const std::wstring& cerPath)
{
    HANDLE f = CreateFileW(cerPath.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE) return false;
    DWORD size = GetFileSize(f, nullptr);
    std::vector<BYTE> data(size);
    DWORD read = 0;
    ReadFile(f, data.data(), size, &read, nullptr);
    CloseHandle(f);
    bool ok = true;
    for (LPCWSTR store : { L"Root", L"TrustedPublisher" })
    {
        HCERTSTORE s = CertOpenStore(CERT_STORE_PROV_SYSTEM_W, 0, 0, CERT_SYSTEM_STORE_LOCAL_MACHINE, store);
        ok = ok && s && CertAddEncodedCertificateToStore(s, X509_ASN_ENCODING | PKCS_7_ASN_ENCODING, data.data(), read, CERT_STORE_ADD_REPLACE_EXISTING, nullptr);
        if (s) CertCloseStore(s, 0);
    }
    return ok;
}

static void RemoveCertificates()
{
    for (LPCWSTR store : { L"Root", L"TrustedPublisher" })
    {
        HCERTSTORE s = CertOpenStore(CERT_STORE_PROV_SYSTEM_W, 0, 0, CERT_SYSTEM_STORE_LOCAL_MACHINE, store);
        if (!s) continue;
        PCCERT_CONTEXT c = nullptr;
        while ((c = CertFindCertificateInStore(s, X509_ASN_ENCODING, 0, CERT_FIND_SUBJECT_STR_W, L"NOTHING CONNECT Display", nullptr)) != nullptr)
        {
            CertDeleteCertificateFromStore(c); // libère aussi le contexte
        }
        CertCloseStore(s, 0);
    }
}

static void WINAPI OnCreated(HSWDEVICE, HRESULT hr, PVOID ctx, PCWSTR)
{
    auto* p = (std::pair<HANDLE, HRESULT>*)ctx;
    p->second = hr;
    SetEvent(p->first);
}

static bool IsTrustError(DWORD e)
{
    // Erreurs de signature / confiance renvoyées par l'installation PnP.
    return e == ERROR_INVALID_IMAGE_HASH || e == 0xE0000242 /* ERROR_AUTHENTICODE_PUBLISHER_NOT_TRUSTED */ ||
           e == 0xE0000244 /* ERROR_AUTHENTICODE_TRUST_NOT_ESTABLISHED */ || e == 0x800B0109 /* CERT_E_UNTRUSTEDROOT */ ||
           e == 0x800B0100 /* TRUST_E_NOSIGNATURE */ || e == 0xE000024B /* ERROR_DRIVER_STORE_ADD_FAILED */;
}

static int Install(const std::wstring& dir)
{
    // Pilote en mode utilisateur (UMDF) : un certificat approuvé localement suffit en général.
    // Si Windows exige malgré tout une signature Microsoft, on le signale (code 10).
    AddCertificate(dir + L"\\NcDisplay.cer");
    std::wstring inf = dir + L"\\NcDisplay.inf";
    BOOL reboot = FALSE;
    if (!DiInstallDriverW(nullptr, inf.c_str(), 0, &reboot))
    {
        DWORD e = GetLastError();
        fwprintf(stderr, L"Installation du pilote refusée (0x%08lx).\n", e);
        return (IsTrustError(e) && !TestSigningEnabled()) ? 10 : 1;
    }
    if (!InterfacePresent())
    {
        // Périphérique logiciel racine, conservé après la fermeture de ce programme.
        std::pair<HANDLE, HRESULT> ctx(CreateEventW(nullptr, TRUE, FALSE, nullptr), E_FAIL);
        SW_DEVICE_CREATE_INFO info = { sizeof(info) };
        info.pszInstanceId = NC_DISPLAY_HWID;
        info.pszzHardwareIds = L"NothingConnectDisplay\0\0";
        info.pszzCompatibleIds = L"NothingConnectDisplay\0\0";
        info.pszDeviceDescription = L"NOTHING CONNECT - écran du téléphone";
        info.CapabilityFlags = SWDeviceCapabilitiesRemovable | SWDeviceCapabilitiesSilentInstall | SWDeviceCapabilitiesDriverRequired;
        HSWDEVICE h = nullptr;
        HRESULT hr = SwDeviceCreate(NC_DISPLAY_HWID, L"HTREE\\ROOT\\0", &info, 0, nullptr, OnCreated, &ctx, &h);
        if (FAILED(hr))
        {
            fwprintf(stderr, L"Création du périphérique impossible (0x%08lx).\n", hr);
            return 1;
        }
        WaitForSingleObject(ctx.first, 15000);
        SwDeviceSetLifetime(h, SWDeviceLifetimeParentPresent);
        SwDeviceClose(h);
        CloseHandle(ctx.first);
        if (FAILED(ctx.second)) return 1;
    }
    return reboot ? 12 : 0;
}

static int Uninstall()
{
    HDEVINFO h = SetupDiGetClassDevsW(nullptr, nullptr, nullptr, DIGCF_ALLCLASSES);
    if (h == INVALID_HANDLE_VALUE) return 1;
    SP_DEVINFO_DATA d = { sizeof(d) };
    std::vector<std::wstring> infs;
    for (DWORD i = 0; SetupDiEnumDeviceInfo(h, i, &d); i++)
    {
        wchar_t ids[1024] = {};
        if (!SetupDiGetDeviceRegistryPropertyW(h, &d, SPDRP_HARDWAREID, nullptr, (PBYTE)ids, sizeof(ids), nullptr)) continue;
        bool match = false;
        for (wchar_t* p = ids; *p; p += wcslen(p) + 1) match = match || _wcsicmp(p, NC_DISPLAY_HWID) == 0;
        if (!match) continue;
        HKEY k = SetupDiOpenDevRegKey(h, &d, DICS_FLAG_GLOBAL, 0, DIREG_DRV, KEY_READ);
        if (k != INVALID_HANDLE_VALUE)
        {
            wchar_t inf[MAX_PATH] = {};
            DWORD sz = sizeof(inf);
            if (RegQueryValueExW(k, L"InfPath", nullptr, nullptr, (LPBYTE)inf, &sz) == ERROR_SUCCESS) infs.push_back(inf);
            RegCloseKey(k);
        }
        BOOL reboot = FALSE;
        DiUninstallDevice(nullptr, h, &d, 0, &reboot);
    }
    SetupDiDestroyDeviceInfoList(h);
    for (const auto& inf : infs)
    {
        SetupUninstallOEMInfW(inf.c_str(), SUOI_FORCEDELETE, nullptr);
    }
    RemoveCertificates();
    return 0;
}

static int EnableTestSigning()
{
    wchar_t sys[MAX_PATH];
    GetSystemDirectoryW(sys, MAX_PATH);
    std::wstring cmd = std::wstring(L"\"") + sys + L"\\bcdedit.exe\" /set testsigning on";
    STARTUPINFOW si = { sizeof(si) };
    PROCESS_INFORMATION pi = {};
    std::vector<wchar_t> buf(cmd.begin(), cmd.end());
    buf.push_back(0);
    if (!CreateProcessW(nullptr, buf.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW, nullptr, nullptr, &si, &pi)) return 1;
    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    // bcdedit échoue quand le démarrage sécurisé (Secure Boot) protège la configuration.
    return code == 0 ? 12 : 11;
}

int wmain(int argc, wchar_t** argv)
{
    std::wstring cmd = argc > 1 ? argv[1] : L"status";
    if (cmd == L"status")
    {
        bool present = InterfacePresent();
        wprintf(L"{\"installed\":%s,\"testsigning\":%s}\n", present ? L"true" : L"false", TestSigningEnabled() ? L"true" : L"false");
        return present ? 0 : 2;
    }
    if (cmd == L"uninstall-if-present")
    {
        return InterfacePresent() ? RelaunchElevated(L"uninstall") : 0;
    }
    if (!IsElevated())
    {
        std::wstring args;
        for (int i = 1; i < argc; i++) args += std::wstring(L"\"") + argv[i] + L"\" ";
        return RelaunchElevated(args);
    }
    if (cmd == L"install" && argc > 2) return Install(argv[2]);
    if (cmd == L"uninstall") return Uninstall();
    if (cmd == L"enable-testsigning") return EnableTestSigning();
    fwprintf(stderr, L"Commande inconnue.\n");
    return 1;
}
