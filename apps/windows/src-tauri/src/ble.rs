//! Annonce Bluetooth Low Energy : aide le téléphone à trouver le PC quand la découverte
//! réseau (mDNS) est bloquée par le routeur. Ne transporte aucun secret ni aucune donnée :
//! seulement l'empreinte publique du PC et son adresse sur le réseau local.
//!
//! Données constructeur (identifiant 0xFFFF, réservé aux tests/usages non commerciaux) :
//! `"NC"` + 8 octets d'empreinte + IPv4 (4) + port (2, big-endian) = 16 octets.

#[cfg_attr(not(windows), allow(dead_code))]
pub const COMPANY_ID: u16 = 0xFFFF;

pub fn payload(fingerprint_hex: &str, ip: std::net::Ipv4Addr, port: u16) -> Option<Vec<u8>> {
    let fp: String = fingerprint_hex
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    if fp.len() != 16 {
        return None;
    }
    let mut v = b"NC".to_vec();
    for i in 0..8 {
        v.push(u8::from_str_radix(&fp[i * 2..i * 2 + 2], 16).ok()?);
    }
    v.extend_from_slice(&ip.octets());
    v.extend_from_slice(&port.to_be_bytes());
    Some(v)
}

#[cfg(windows)]
pub struct Advertiser(
    windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementPublisher,
);

#[cfg(windows)]
impl Advertiser {
    pub fn start(data: &[u8]) -> windows::core::Result<Self> {
        use windows::Devices::Bluetooth::Advertisement::{
            BluetoothLEAdvertisementPublisher, BluetoothLEManufacturerData,
        };
        use windows::Storage::Streams::DataWriter;
        let publisher = BluetoothLEAdvertisementPublisher::new()?;
        let md = BluetoothLEManufacturerData::new()?;
        md.SetCompanyId(COMPANY_ID)?;
        let w = DataWriter::new()?;
        w.WriteBytes(data)?;
        md.SetData(&w.DetachBuffer()?)?;
        publisher.Advertisement()?.ManufacturerData()?.Append(&md)?;
        publisher.Start()?;
        Ok(Self(publisher))
    }
}

#[cfg(windows)]
impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.0.Stop();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn payload_layout() {
        let p = super::payload(
            "0A51-71C3-74DC-CF58",
            "192.168.1.20".parse().unwrap(),
            47820,
        )
        .unwrap();
        assert_eq!(p.len(), 16);
        assert_eq!(&p[..2], b"NC");
        assert_eq!(p[2], 0x0A);
        assert_eq!(&p[10..14], &[192, 168, 1, 20]);
        assert_eq!(u16::from_be_bytes([p[14], p[15]]), 47820);
        assert!(super::payload("zz", "1.2.3.4".parse().unwrap(), 1).is_none());
    }
}
