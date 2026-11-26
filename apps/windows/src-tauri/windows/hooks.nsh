; Hooks NSIS de NOTHING CONNECT.
; Installation par utilisateur (aucun droit administrateur). Les mises à jour s'installent
; par-dessus la version précédente : données et appareils associés sont conservés.

!macro NSIS_HOOK_POSTINSTALL
  ; Entrée « Envoyer vers > NOTHING CONNECT » dans l'Explorateur.
  CreateShortCut "$APPDATA\Microsoft\Windows\SendTo\NOTHING CONNECT.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Pilote d'écran étendu : retiré seulement s'il est installé (une fenêtre d'autorisation
  ; Windows s'affiche alors). Ne s'exécute pas lors d'une mise à jour.
  ${If} $UpdateMode <> 1
    IfFileExists "$INSTDIR\driver\NcDisplaySetup.exe" 0 +2
      ExecWait '"$INSTDIR\driver\NcDisplaySetup.exe" uninstall-if-present'
  ${EndIf}
  Delete "$APPDATA\Microsoft\Windows\SendTo\NOTHING CONNECT.lnk"
  ; Démarrage automatique éventuellement activé dans l'application.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NOTHING CONNECT"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${MAINBINARYNAME}"
  ; Si l'utilisateur a coché « Supprimer les données de l'application » : efface aussi la
  ; clé d'identité stockée dans le Gestionnaire d'identification Windows.
  ${If} $DeleteAppDataCheckboxState = 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --purge-secrets'
  ${EndIf}
!macroend
