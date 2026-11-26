//! Pare-feu Windows : sur un réseau « Public » (partage de connexion, Wi-Fi public), Windows
//! bloque par défaut les connexions entrantes vers NOTHING CONNECT, et le téléphone ne peut
//! ni s'associer ni se connecter. Diagnostic sans droits ; correction avec une seule
//! autorisation administrateur (règle « NOTHING CONNECT », retirée par le désinstalleur).
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct FirewallStatus {
    /// Diagnostic possible (Windows uniquement).
    pub supported: bool,
    /// Un réseau actif est classé « Public ».
    pub public_network: bool,
    /// Une règle autorise NOTHING CONNECT en entrée sur les réseaux publics.
    pub allowed_public: bool,
    /// Une règle (souvent créée en refusant la fenêtre de Windows) bloque NOTHING CONNECT.
    pub blocked: bool,
    /// Il faut corriger : réseau public non autorisé, ou règle de blocage.
    pub needs_fix: bool,
}

/// Échappe une chaîne pour un littéral PowerShell entre apostrophes.
#[cfg_attr(not(windows), allow(dead_code))]
fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// Interprète la sortie « catégories|autorisé public|bloqué » du script de diagnostic.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_status(out: &str) -> FirewallStatus {
    let line = out.lines().rev().find(|l| l.contains('|')).unwrap_or("");
    let mut it = line.trim().split('|');
    let cats = it.next().unwrap_or("");
    let allowed_public = it
        .next()
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let blocked = it
        .next()
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let public_network = cats
        .split(',')
        .any(|c| c.trim().eq_ignore_ascii_case("public"));
    FirewallStatus {
        supported: true,
        public_network,
        allowed_public,
        blocked,
        needs_fix: blocked || (public_network && !allowed_public),
    }
}

#[cfg(windows)]
fn powershell(script: &str) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .output()
}

#[cfg(windows)]
pub fn status() -> FirewallStatus {
    let Ok(exe) = std::env::current_exe() else {
        return FirewallStatus::default();
    };
    let script = format!(
        r#"$ErrorActionPreference='SilentlyContinue'
$cats = (Get-NetConnectionProfile | Select-Object -ExpandProperty NetworkCategory) -join ','
$rules = Get-NetFirewallApplicationFilter -Program '{exe}' | Get-NetFirewallRule | Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True' }}
$allow = [bool]($rules | Where-Object {{ $_.Action -eq 'Allow' -and ("$($_.Profile)" -match 'Any|Public') }})
$block = [bool]($rules | Where-Object {{ $_.Action -eq 'Block' }})
"$cats|$allow|$block""#,
        exe = ps_quote(&exe.to_string_lossy())
    );
    match powershell(&script) {
        Ok(o) => parse_status(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => FirewallStatus::default(),
    }
}

/// Remplace toutes les règles visant NOTHING CONNECT (y compris un blocage créé par la
/// fenêtre de Windows) par une seule règle d'autorisation, pour tous les types de réseau.
#[cfg(windows)]
pub fn allow() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    let cmd = format!(
        "/c netsh advfirewall firewall delete rule name=all program=\"{exe}\" & netsh advfirewall firewall delete rule name=\"NOTHING CONNECT\" & netsh advfirewall firewall add rule name=\"NOTHING CONNECT\" dir=in action=allow program=\"{exe}\" enable=yes profile=any"
    );
    let script = format!(
        "$p = Start-Process -FilePath cmd.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '{}'; exit $p.ExitCode",
        ps_quote(&cmd)
    );
    let out = powershell(&script).map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else if String::from_utf8_lossy(&out.stderr)
        .to_lowercase()
        .contains("cancel")
        || String::from_utf8_lossy(&out.stderr)
            .to_lowercase()
            .contains("annul")
    {
        Err("Autorisation refusée : le pare-feu n'a pas été modifié.".into())
    } else {
        Err("Le pare-feu n'a pas pu être modifié.".into())
    }
}

#[cfg(not(windows))]
pub fn status() -> FirewallStatus {
    FirewallStatus::default()
}

#[cfg(not(windows))]
pub fn allow() -> Result<(), String> {
    Err("Disponible uniquement sous Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_network_without_rule_needs_fix() {
        let s = parse_status("Public|False|False\r\n");
        assert!(s.public_network && !s.allowed_public && s.needs_fix);
        let s = parse_status("Private,Public|True|False");
        assert!(!s.needs_fix);
        let s = parse_status("Private|False|False");
        assert!(!s.needs_fix);
        // Règle de blocage : à corriger même sur un réseau privé.
        assert!(parse_status("Private|True|True").needs_fix);
        assert_eq!(
            ps_quote("C:\\Users\\l'ami\\a.exe"),
            "C:\\Users\\l''ami\\a.exe"
        );
    }
}
