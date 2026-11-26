# Compile le pilote d'écran et l'outil d'installation, crée le catalogue et le signe avec un
# certificat de TEST (la signature Microsoft, nécessaire pour une installation sans mode test,
# exige un certificat EV et le portail Partner Center : voir docs/pilote-ecran.md).
# Sortie : apps/windows/src-tauri/driver/ (embarqué dans l'installeur Windows).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$out = Join-Path $root "..\..\..\apps\windows\src-tauri\driver"
$msbuild = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1

& $msbuild "$root\NcDisplay\NcDisplay.vcxproj" /p:Configuration=Release /p:Platform=x64 /m /nologo /v:minimal
if ($LASTEXITCODE -ne 0) { throw "compilation du pilote échouée" }
& $msbuild "$root\NcDisplaySetup\NcDisplaySetup.vcxproj" /p:Configuration=Release /p:Platform=x64 /m /nologo /v:minimal
if ($LASTEXITCODE -ne 0) { throw "compilation de l'outil d'installation échouée" }

$pkg = Join-Path $root "NcDisplay\x64\Release\NcDisplay"
if (-not (Test-Path "$pkg\NcDisplay.inf")) { $pkg = Join-Path $root "NcDisplay\x64\Release" }
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item "$pkg\NcDisplay.inf", (Join-Path $root "NcDisplay\x64\Release\NcDisplay.dll") -Destination $out -Force
Copy-Item (Join-Path $root "NcDisplaySetup\x64\Release\NcDisplaySetup.exe") -Destination $out -Force
Copy-Item (Join-Path $root "NcDisplay\LICENSE-MS-PL.txt") -Destination $out -Force

$kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
$ver = (Get-ChildItem $kits -Directory | Where-Object { $_.Name -match '^10\.' } | Sort-Object Name -Descending | Select-Object -First 1).Name
$inf2cat = "$kits\$ver\x86\Inf2Cat.exe"
if (-not (Test-Path $inf2cat)) { $inf2cat = "$kits\x86\Inf2Cat.exe" }
$signtool = "$kits\$ver\x64\signtool.exe"
& $inf2cat /driver:$out /os:10_X64,10_VB_X64,10_NI_X64 /uselocaltime
if ($LASTEXITCODE -ne 0) { throw "Inf2Cat a échoué" }

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=NOTHING CONNECT Display Test" -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(5)
$pwd = ConvertTo-SecureString -String ([guid]::NewGuid().ToString()) -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath "$env:TEMP\nc-test.pfx" -Password $pwd | Out-Null
Export-Certificate -Cert $cert -FilePath "$out\NcDisplay.cer" | Out-Null
$plain = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($pwd))
& $signtool sign /fd sha256 /f "$env:TEMP\nc-test.pfx" /p $plain "$out\NcDisplay.dll" "$out\NcDisplay.cat" "$out\NcDisplaySetup.exe"
if ($LASTEXITCODE -ne 0) { throw "signature échouée" }
Remove-Item "$env:TEMP\nc-test.pfx"
Get-ChildItem $out
