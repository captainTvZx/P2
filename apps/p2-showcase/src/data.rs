//! Vitrin için deterministik sahte veri (her açılışta aynı).

pub struct Person {
    pub name: &'static str,
    pub status: Status,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Online,
    Away,
    Busy,
    Offline,
}

pub const PEOPLE: &[Person] = &[
    Person { name: "Hasan Özoğlu", status: Status::Online },
    Person { name: "Cem Akkaya", status: Status::Online },
    Person { name: "Aslı Demir", status: Status::Away },
    Person { name: "Mert Kaya", status: Status::Busy },
    Person { name: "Elif Şahin", status: Status::Online },
    Person { name: "Yusuf Çelik", status: Status::Online },
    Person { name: "Zeynep Arslan", status: Status::Offline },
    Person { name: "Kenji Sato", status: Status::Online },
    Person { name: "Li Wei", status: Status::Away },
    Person { name: "Burak Yıldız", status: Status::Online },
    Person { name: "Selin Koç", status: Status::Offline },
    Person { name: "Emre Aydın", status: Status::Online },
];

pub const CHANNELS: &[&str] = &["genel", "tasarım", "derleme-hataları", "sürüm-notları", "rastgele"];

pub struct Message {
    pub author: usize,
    pub minute: u32,
    pub body: String,
}

const LINES: &[&str] = &[
    "Yeni derleme hazır, **beta** kanalına attım 🚀",
    "@cem çekici fiziğinde halat hâlâ titriyor, bakabilir misin?",
    "Bence menü geçişleri biraz daha *yumuşak* olmalı.",
    "Tamamdır 👍",
    "Şu hatayı alıyorum: `NullReferenceException at TowHook.Attach()`",
    "Sabah toplantısı 10:30'da, gecikmeyin ☕",
    "Shader derlemesi 4 dakikadan 40 saniyeye indi, `DDC` önbelleği açınca.",
    "@asli yeni ikon setini Figma'ya yükledim, göz atar mısın? 🎨",
    "Haftaya kadar **1950s Tow Truck** demosunu bitirmemiz lazım.",
    "日本語のテストメッセージです。レンダリングは大丈夫？",
    "中文测试：字体回退正常吗？",
    "Ğğ Üü Şş İı Öö Çç — Türkçe karakterler düzgün görünüyor mu?",
    "Lol 😂😂",
    "Patch notlarını `sürüm-notları` kanalına yazdım.",
    "@mert Unreal 5.6'ya geçince **Nanite** ayarlarını kontrol et.",
    "Bir şey sormak istiyorum: lightmap çözünürlüğünü kim değiştirdi? 🤔",
    "Oyun içi overlay'de ekran görüntüsü kısayolu çalışıyor ✅",
    "Kod incelemesi bekleyen 3 PR var, *öğleden sonra* bakarım.",
];

const LONG: &str = "Uzun bir mesaj örneği: kamyonun süspansiyonu yeniden ayarlandı, \
artık bozuk yolda daha az zıplıyor. Bunun yanında motor sesi örnekleri değiştirildi, \
vites geçişleri daha gerçekçi. Bir de ön tamponun çarpışma kutusu küçültüldü çünkü \
dar sokaklarda takılıyordu. Test edip geri bildirim verirseniz sevinirim 🙏 @burak @elif";

const CODE: &str = "Şu fonksiyon yavaş, profil sonucu:\n```rust\nfn attach_hook(truck: &mut Truck, car: &Car) -> Result<()> {\n    let anchor = car.tow_points().nearest(truck.hook())?;\n    truck.rope.connect(anchor, ROPE_LENGTH)\n}\n```\nNe dersiniz?";

/// xorshift: tekrarlanabilir sahte rastgelelik.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

pub fn messages(channel: usize, count: usize) -> Vec<Message> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (channel as u64 + 1) * 0x1000_0001);
    let mut minute = 8 * 60;
    (0..count)
        .map(|i| {
            minute += rng.below(4) as u32;
            let body = match rng.below(40) {
                0 => LONG.to_string(),
                1 => CODE.to_string(),
                _ => LINES[rng.below(LINES.len())].to_string(),
            };
            Message { author: if i % 7 == 0 { 0 } else { rng.below(PEOPLE.len()) }, minute, body }
        })
        .collect()
}

pub fn clock(minute: u32) -> String {
    format!("{:02}:{:02}", (minute / 60) % 24, minute % 60)
}

pub fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect()
}
