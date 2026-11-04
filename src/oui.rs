//! Hersteller-Erkennung über die OUI (erste 3 Bytes der BSSID).
//!
//! Eine kompakte Basis-Liste typischer WLAN-Hersteller ist eingebaut. Über den Button
//! "Hersteller-DB laden" kann die vollständige Liste (Wireshark manuf bzw. IEEE) geladen
//! werden; sie wird unter %APPDATA%\WifiAnalyzer\oui.txt zwischengespeichert.

use std::collections::HashMap;
use std::path::PathBuf;

const BUILTIN: &str = "\
Ubiquiti|00156D 002722 0418D6 18E829 24A43C 44D9E7 687251 7483C2 788A20 802AA8 B4FBE4 DC9FDB E063DA F09FC2 FCECDA 245A4C 602232 70A741 9C05D6 AC8BA9 D8B370 E43883 1C6A1B 28704E 0CEA14 942A6F 74ACB9 784558 F492BF 847848 6C63F8 D021F9
AVM (FRITZ!)|00040E 001C4A 001F3F 0024FE 0896D7 246511 2C3AFD 2C91AB 3431C4 3810D5 3CA62F 444E6D 5C4979 7CFF4D 989BCB 9CC7A6 BC0543 C02506 C80E14 CCCE1E DC396F E0286D E8DF70 F0B014 74427F 50E636 B0F208 1CED6F 3C3712 60B58D 04B4FE D424DD
Cisco|004096 000B85 000C85 00173B 001A30 001B2A 001D45 001E13 002290 00245F 0026CB 00F663 00A2EE 00425A 3C0E23 58971E 5897BD 70105C 70DB98 74A2E6 7C95F3 88F031 F4DBE6 2C3F38 687DB4 706D15 4C776D 0CF5A4 188B9D 34A84E 6C710D A0E0AF 00270D
Cisco Meraki|00180A 0C8DDB 881544 E0553D 3456FE AC17C8 E0CBBC 981888 683A1E 2C3F0B
Aruba (HPE)|000B86 001A1E 00246C 04BD88 186472 204C03 24DEC6 40E3D6 6CF37F 703A0E 84D47E 94B40F 9C1C12 ACA31E B45D50 D8C7C8 F05C19 34FCB9 883A30 20A6CD 3817C3 484AE9 904C81 B83A5A EC50AA 004E35
Ruckus|001F41 00227F 002482 0025C4 044FAA 0CF4D5 2C5D93 58B633 74911A 84183A C4017C C08ADE D4684D EC58EA F0B052 543D37 8C7A15 38FF36 24792A 94BFC4
MikroTik|000C42 18FD74 2CC81B 488F5A 4C5E0C 64D154 6C3B6B 744D28 B869F4 C4AD34 CC2DE0 D4CA6D DC2C6E E48D8C 085531 789A18 48A98A F41E57 D401C3
TP-Link|6083E7 202351 002719 14CC20 14CF92 18A6F7 1C3BF3 30B5C2 50C7BF 54C80F 60E327 6466B3 647002 74DA88 8416F9 90F652 98DAC4 A0F3C1 AC84C6 B04E26 B09575 C006C3 C46E1F C04A00 D807B6 E848B8 EC086B F4F26D F81A67 3C846A 5091E3 9CA2F4 5CA6E6 1C61B4 2887BA 30DE4B 482254 68FF7B 788CB5 A842A1 B4B024 3C52A1 5CE931 60A4B7 98254A D84489 E4FAC4 1027F5 242FD0 3460F9 40ED00 54AF97 6032B1 7CC2C6 9C5322 A86E84 B01921 C0C9E3 CC32E5 D84732
Netgear|00095B 00146C 001B2F 001E2A 001F33 00223F 0024B2 0026F2 204E7F 28C68E 2CB05D 30469A 4494FC 4C60DE 6CB0CE 841B5E 9C3DCF A00460 A040A0 A42B8C B07FB9 C03F0E C40415 CC40D0 E0469A E091F5 E4F4C6 08028E 100D7F 94A67E 3894ED 3C3786 80CC9C 9CC9EB BCA511 78D294
ASUS|000C6E 00112F 0015F2 001731 001A92 001D60 001E8C 002215 002354 00248C 002618 049226 08606E 107B44 14DAE9 1C872C 2C4D54 2C56DC 305A3A 382C4A 40167E 4CEDFB 50465D 5404A6 6045CB 704D7B 74D02B 88D7F6 90E6BA AC220B AC9E17 BCEE7B C86000 D850E6 E03F49 F07959 F46D04 F832E4 04D4C4 244BFE 0C9D92 7C10C9 A85E45 FC3497 3C7C3F 581122 1831BF 08BFB8
Huawei|001882 001E10 00259E 00464B 00E0FC 04C06F 0819A6 0C37DC 101B54 20F3A3 286ED4 346BD3 4846FB 4C1FCC 548998 5C7D5E 70723C 80B686 8853D4 ACE215 BC7670 C8D15E D46E5C E8CD2D F4C714 009ACD 24DBAC 48DB50 6416F0
Apple|000393 001CB3 3C0754 40A6D9 60FB42 7CD1C3 88665A A4B1C1 ACBC32 D023DB F0B479 28CFE9 705681 8C8590 B8E856 F01898
Samsung|001247 001599 001632 001D25 002119 002339 002637 08D42B 3423BA 38AA3C 5C0A5B 8425DB 8C7712 90187C BC72B1 C4576E E8508B F025B7 0007AB
Google|F4F5D8 F4F5E8 546009 3C5AB4 1CF29A 883D24 D86C63 20DFB9 48D6D5 CCF411 A47733
Amazon|00FC8B 0C47C9 34D270 40B4CD 44650D 6837E9 74C246 84D6D0 A002DC F0272D FC65DE 50DCE7
Intel|0013E8 001B77 001E64 00216A 0024D7 3CA9F4 4851B7 606720 7C5CF8 8086F2 A44E31 B46BFC DC5360 F81654 8C8D28 3413E8
D-Link|00055D 000D88 000F3D 001195 001346 0015E9 00179A 00195B 001B11 001CF0 001E58 002191 0022B0 002401 00265A 1C7EE5 28107B 340804 5CD998 78542E 84C9B2 9094E4 B8A386 BCF685 C4A81D C8BE19 CCB255 F07D68 FC7516
Linksys|000625 000C41 001217 001310 0014BF 0016B6 001839 0018F8 001A70 001C10 001D7E 001EE5 002129 00226B 002369 00259C 20AA4B 48F8B3 586D8F 687F74 C0C1C0 149182 EC1A59
Belkin|24F5A2 94103E 08863B C05627
Zyxel|001349 0019CB 0023F8 00A0C5 107BEF 404A03 5CF4AB 90EF68 B8ECA3 BC9911 C86C87 CC5D4E E4186B FCF528 4C9EFF 588BF3
LANCOM|00A057
Fortinet|00090F 085B0E 704CA5 906CAC E81CBA 04D590
Juniper Mist|5C5B35 D420B0
Juniper|000585
Extreme Networks|B4C799 000496 00E02B
Aerohive|885BDD 001977 08EA44 9C5D12 4018B1
Cambium|58C17A 000456
Grandstream|000B82 C074AD EC74D7
EnGenius|00026F 88DC96
Sophos|001A8C 7C5A1C
Zebra/Symbol|00A0F8 001570 002368 84248D
Xiaomi|286C07 34CE00 50642B 640980 7811DC 8CBEBE 9C9D7E F8A45F 50EC50 7C49EB 04CF8C
Tenda|C83A35 502B73 0495E6 D83214
Synology|001132
DrayTek|001DAA 1449BC
Raspberry Pi|B827EB DCA632 E45F01 D83ADD 28CDC1 2CCF67
Espressif (IoT)|240AC4 246F28 30AEA4 3C71BF 840D8E A4CF12 BCDDC2 CC50E3 ECFABC 5CCF7F 600194 18FE34 84F3EB 8CAAB5 98F4AB C82B96 DC4F22 E8DB84 40F520 483FDA 7C9EBD A020A6 B4E62D 083AF2 2462AB 308398 349454 441793 485519 58BF25 70039F 807D3A 94B97E C44F33 C45BBE E09806 F4CFA2 10521C 2CF432 3C6105 4C11AE 68C63A 8CCE4E A848FA BCFF4D C4DD57 D8BFC0 E868E7 F008D1
Sonos|000E58 5CAAFD 7828CA 949F3E B8E937 48A6B8 347E5C
HP|3CD92B 9C8E99 00215A 0025B3 10604B 705A0F A0D3C1
Epson|0026AB 64EB8C A4EE57 381A52 AC1826
Brother|001BA9 30055C 3C2AF4 008077
Canon|001E8F 180CAC 2C9EFC
Siemens|000E8C 001B1B 208756 001FF8
Hirschmann|008063 ECE555
Moxa|0090E8
Phoenix Contact|00A045
Teltonika|001E42
Edimax|000E2E 74DA38 801F02
devolo|000B3B F4068D 30D32D BCF2AF
Arcadyan (Speedport/Vodafone)|0012BF 001A2A 001D19 002308 1CC63C 4C09D4 7C4FB5 880355 849CA6 1883BF 743170 00264D 9C80DF A8D3F7 D0052A E0CA94
Sercomm|00C002
Ruijie|00749C 58696C 800588
";

pub struct OuiDb {
    map: HashMap<u32, String>,
    pub full_db: bool,
}

fn key(b: &[u8]) -> u32 {
    ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32
}

/// Kürzt lange Firmennamen ("XYZ Technologies Co., Ltd." -> "XYZ Technologies")
fn shorten(name: &str) -> String {
    let mut s = name.trim().trim_matches('"').to_string();
    for suf in [
        ", Inc.", ", Inc", " Inc.", " Inc", " Co., Ltd.", " Co.,Ltd.", " Co., Ltd", " CO., LTD.",
        " Co.,Ltd", " Ltd.", " Ltd", " LLC", " GmbH", " AG", " Corporation", " Corp.", " Corp",
        " Limited", " S.A.", " B.V.", ",",
    ] {
        if s.len() > suf.len() + 2 && s.ends_with(suf) {
            s.truncate(s.len() - suf.len());
        }
    }
    s.trim().to_string()
}

fn parse_hex_prefix(s: &str) -> Option<u32> {
    let h: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if h.len() != 6 {
        return None;
    }
    u32::from_str_radix(&h, 16).ok()
}

/// Einfache CSV-Feldtrennung mit Anführungszeichen
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in line.chars() {
        match c {
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

impl OuiDb {
    pub fn load() -> Self {
        let mut db = Self { map: HashMap::new(), full_db: false };
        for line in BUILTIN.lines() {
            if let Some((name, list)) = line.split_once('|') {
                for h in list.split_whitespace() {
                    if let Ok(k) = u32::from_str_radix(h, 16) {
                        db.map.insert(k, name.to_string());
                    }
                }
            }
        }
        if let Ok(text) = std::fs::read_to_string(Self::cache_path()) {
            if db.parse_external(&text) > 1000 {
                db.full_db = true;
            }
        }
        db
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn cache_path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("WifiAnalyzer").join("oui.txt")
    }

    /// Versteht Wireshark-"manuf", IEEE "oui.csv" und IEEE "oui.txt".
    fn parse_external(&mut self, text: &str) -> usize {
        let mut n = 0;
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("MA-L,") {
                let f = csv_fields(rest);
                if f.len() >= 2 {
                    if let Some(k) = parse_hex_prefix(&f[0]) {
                        self.map.insert(k, shorten(&f[1]));
                        n += 1;
                    }
                }
            } else if line.contains("(hex)") {
                let mut parts = line.splitn(2, "(hex)");
                if let (Some(p), Some(name)) = (parts.next(), parts.next()) {
                    if let Some(k) = parse_hex_prefix(p) {
                        self.map.insert(k, shorten(name));
                        n += 1;
                    }
                }
            } else if line.contains('\t') {
                let f: Vec<&str> = line.split('\t').collect();
                // nur volle 24-bit OUIs (keine /28, /36)
                if f.len() >= 2 && f[0].len() == 8 {
                    if let Some(k) = parse_hex_prefix(f[0]) {
                        let name = if f.len() >= 3 && !f[2].trim().is_empty() { f[2] } else { f[1] };
                        self.map.insert(k, shorten(name));
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Liefert (Hersteller, virtuell/LAA-Kennung)
    pub fn lookup(&self, mac: &[u8; 6]) -> Option<(String, bool)> {
        if let Some(v) = self.map.get(&key(mac)) {
            return Some((v.clone(), false));
        }
        if mac[0] & 0x02 != 0 {
            // Lokal administrierte (virtuelle) BSSID: viele APs setzen nur das LAA-Bit
            let mut m = *mac;
            m[0] &= !0x02;
            if let Some(v) = self.map.get(&key(&m)) {
                return Some((v.clone(), true));
            }
        }
        None
    }
}

/// Lädt die vollständige Hersteller-Datenbank (blockierend, im Hintergrund-Thread aufrufen).
pub fn download() -> Result<usize, String> {
    use std::os::windows::process::CommandExt;
    let path = OuiDb::cache_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let urls = [
        "https://www.wireshark.org/download/automated/data/manuf",
        "https://standards-oui.ieee.org/oui/oui.csv",
    ];
    let mut last_err = String::from("Download fehlgeschlagen");
    for url in urls {
        let ps = format!(
            "$ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol='Tls12,Tls13'; \
             Invoke-WebRequest -UseBasicParsing -UserAgent 'Mozilla/5.0 WifiAnalyzer' -Uri '{}' -OutFile '{}'",
            url,
            tmp.display()
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let text = std::fs::read_to_string(&tmp).unwrap_or_default();
                let mut test = OuiDb { map: HashMap::new(), full_db: false };
                let n = test.parse_external(&text);
                if n > 1000 {
                    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
                    return Ok(n);
                }
                last_err = format!("Ungültige Daten von {url}");
            }
            Ok(o) => last_err = String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("Fehler").to_string(),
            Err(e) => last_err = e.to_string(),
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last_err)
}
