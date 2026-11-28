; Hooks NSIS de NOTHING CONNECT.
; Installation par utilisateur (aucun droit administrateur). Les mises à jour s'installent
; par-dessus la version précédente : données et appareils associés sont conservés.

; ------------------------------------------------------------------ mise à niveau manuelle
; Lancé par-dessus une version installée (double-clic sur le nouvel installateur), Tauri
; propose « Désinstaller avant d'installer », choisi par défaut, et lance alors l'ancien
; désinstalleur SANS /UPDATE. Jusqu'à la 0.5.0, celui-ci retirait donc le pilote d'écran
; étendu (à réinstaller ensuite), la règle du pare-feu, les outils USB et le démarrage
; automatique. Désormais :
;  - l'installateur le signale aux désinstalleurs qu'il lance (variable d'environnement
;    héritée), qui se comportent alors comme une mise à jour ;
;  - il cache l'outil du pilote aux désinstalleurs 0.5.0 et antérieurs, qui ne retirent le
;    pilote que s'ils trouvent cet outil (remis en place en cas d'annulation, et par
;    l'application si l'installateur est interrompu).
Var NcKeepDir
!define MUI_CUSTOMFUNCTION_GUIINIT NcGuiInit
!define MUI_CUSTOMFUNCTION_ABORT NcAbort

Function NcGuiInit
  System::Call 'kernel32::SetEnvironmentVariable(t "NC_SETUP_UPGRADE", t "1")'
  StrCpy $NcKeepDir ""
  ${If} ${FileExists} "$INSTDIR\driver\NcDisplaySetup.exe"
    Delete "$INSTDIR\driver\NcDisplaySetup.exe.keep"
    Rename "$INSTDIR\driver\NcDisplaySetup.exe" "$INSTDIR\driver\NcDisplaySetup.exe.keep"
    StrCpy $NcKeepDir "$INSTDIR"
  ${EndIf}
FunctionEnd

Function NcAbort
  ${If} $NcKeepDir != ""
  ${AndIf} ${FileExists} "$NcKeepDir\driver\NcDisplaySetup.exe.keep"
  ${AndIfNot} ${FileExists} "$NcKeepDir\driver\NcDisplaySetup.exe"
    Rename "$NcKeepDir\driver\NcDisplaySetup.exe.keep" "$NcKeepDir\driver\NcDisplaySetup.exe"
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  ; Entrée « Envoyer vers > NOTHING CONNECT » dans l'Explorateur.
  CreateShortCut "$APPDATA\Microsoft\Windows\SendTo\NOTHING CONNECT.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
  ; Outil du pilote mis de côté pendant la mise à niveau : la nouvelle version l'a remplacé.
  ${If} $NcKeepDir != ""
    Delete "$NcKeepDir\driver\NcDisplaySetup.exe.keep"
    RMDir "$NcKeepDir\driver"
    RMDir "$NcKeepDir"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Lancé par un installateur NOTHING CONNECT (mise à niveau) : comme une mise à jour, on
  ; garde pilote, pare-feu, outils USB, démarrage automatique, raccourcis et données.
  ReadEnvStr $R9 NC_SETUP_UPGRADE
  ${If} $R9 == "1"
    StrCpy $UpdateMode 1
  ${EndIf}
  ; Pilote d'écran étendu : retiré seulement s'il est installé (une fenêtre d'autorisation
  ; Windows s'affiche alors).
  ${If} $UpdateMode <> 1
    IfFileExists "$INSTDIR\driver\NcDisplaySetup.exe" 0 +2
      ExecWait '"$INSTDIR\driver\NcDisplaySetup.exe" uninstall-if-present'
  ${EndIf}
  ; Règle « NOTHING CONNECT » du pare-feu (si l'utilisateur l'a créée depuis l'application) :
  ; retirée avec une fenêtre d'autorisation Windows.
  ${If} $UpdateMode <> 1
    nsExec::ExecToStack 'netsh advfirewall firewall show rule name="NOTHING CONNECT"'
    Pop $0
    Pop $1
    ${If} $0 == 0
      ExecShellWait "runas" "netsh" 'advfirewall firewall delete rule name="NOTHING CONNECT"' SW_HIDE
    ${EndIf}
  ${EndIf}
  ; Outils USB (adb) téléchargés par l'application : arrêt du serveur adb puis suppression.
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --cleanup-usb-tools'
  ${EndIf}
  Delete "$APPDATA\Microsoft\Windows\SendTo\NOTHING CONNECT.lnk"
  ${If} $UpdateMode <> 1
    ; Démarrage automatique éventuellement activé dans l'application.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NOTHING CONNECT"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${MAINBINARYNAME}"
  ${EndIf}
  ; Si l'utilisateur a coché « Supprimer les données de l'application » : efface aussi la
  ; clé d'identité stockée dans le Gestionnaire d'identification Windows.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --purge-secrets'
  ${EndIf}
!macroend
