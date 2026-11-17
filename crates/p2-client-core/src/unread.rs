//! Proje rayındaki okunmamış rozetinin hesabı.
//!
//! Rozet = o projenin **görünen** kanallarındaki okunmamışlar + o projenin (listeden kaldırılmamış,
//! engellenmemiş) birebir sohbetlerindeki okunmamışlar. Kanal sayısı iki yerden gelir:
//!   - seçili olmayan projelerde sunucunun toplamı (`GET /organizations/:org/projects/unread`),
//!   - seçili projede istemcinin elindeki kanal listesi (kullanıcı okudukça sıfırlanır).
//!
//! Eski hata: sunucu toplamı yalnız açılışta ve başka projeye mesaj gelince alınıyordu. Kullanıcı projeyi
//! açıp her şeyi okuduktan sonra başka projeye geçince rozet bellekteki **eski** toplamdan çiziliyor ve
//! "1" hiç kaybolmuyordu. Artık projeden çıkarken o projenin sayısı eldeki kanal listesinden yazılır
//! ([`channel_unread`]) ve sunucudan da tazelenir.

use std::collections::{HashMap, HashSet};

use crate::models::{ChannelCategory, Conversation};

/// Projenin kanal listesindeki okunmamış toplamı: ayrıldığım (gizlediğim) kanallar ve şu an açık olan
/// kanal sayılmaz (açık kanal okunmuş sayılır).
pub fn channel_unread(categories: &[ChannelCategory], hidden: &HashSet<String>, open: Option<&str>) -> u32 {
    categories.iter().flat_map(|c| c.channels.iter()).filter(|c| !hidden.contains(&c.id) && open != Some(c.id.as_str())).map(|c| c.unread).sum()
}

/// Projenin birebir sohbetlerindeki okunmamış toplamı: listeden kaldırılan, engellediğim kişilerle olan
/// (ya da `blocked` kümesindeki kişilerle olan) ve şu an açık olan sohbet sayılmaz.
pub fn conversation_unread(conversations: &[Conversation], project: &str, blocked: &HashSet<String>, open: Option<&str>) -> u32 {
    conversations
        .iter()
        .filter(|c| c.project_id.as_deref() == Some(project) && !c.hidden && !c.blocked_by_me && !blocked.contains(&c.user.id) && open != Some(c.id.as_str()))
        .map(|c| c.unread)
        .sum()
}

/// Projeden çıkarken (başka projeye / Ana sayfaya geçerken) bellekteki sunucu toplamını eldeki kanal
/// listesiyle değiştirir: okunan her şey sıfırlanmış, okunmayanlar olduğu gibi kalmış olur.
pub fn stash_channel_unread(totals: &mut HashMap<String, u32>, project: &str, categories: &[ChannelCategory], hidden: &HashSet<String>, open: Option<&str>) -> u32 {
    let n = channel_unread(categories, hidden, open);
    if n == 0 {
        totals.remove(project);
    } else {
        totals.insert(project.to_string(), n);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn categories(unread: &[(&str, u32)]) -> Vec<ChannelCategory> {
        let channels: Vec<serde_json::Value> = unread.iter().map(|(id, n)| serde_json::json!({ "id": id, "name": id, "unread": n })).collect();
        serde_json::from_value(serde_json::json!([{ "id": "cat", "name": "GENEL", "channels": channels }])).unwrap()
    }

    fn conversation(id: &str, user: &str, project: Option<&str>, unread: u32, hidden: bool, blocked: bool) -> Conversation {
        serde_json::from_value(serde_json::json!({
            "id": id, "unread": unread, "projectId": project, "hidden": hidden, "blockedByMe": blocked,
            "user": { "id": user, "username": user, "displayName": user }
        }))
        .unwrap()
    }

    #[test]
    fn okunan_proje_rozeti_sifirlanir() {
        // Sunucudan gelen eski toplam: proje A'da 1 okunmamış.
        let mut totals = HashMap::from([("A".to_string(), 1u32)]);
        // Kullanıcı A'yı açtı, kanalı okudu (kanal listesinde hepsi 0), sonra başka projeye geçti.
        let cats = categories(&[("genel", 0), ("duyuru", 0)]);
        assert_eq!(stash_channel_unread(&mut totals, "A", &cats, &HashSet::new(), None), 0);
        assert!(!totals.contains_key("A"), "her şey okununca rozet kalmamalı");
    }

    #[test]
    fn okunmayan_kanal_rozette_kalir() {
        let mut totals = HashMap::from([("A".to_string(), 5u32)]);
        let cats = categories(&[("genel", 0), ("duyuru", 2), ("sesli", 1)]);
        assert_eq!(stash_channel_unread(&mut totals, "A", &cats, &HashSet::new(), None), 3);
        assert_eq!(totals.get("A"), Some(&3));
    }

    #[test]
    fn gizli_ve_acik_kanal_sayilmaz() {
        let cats = categories(&[("genel", 4), ("ayrildigim", 7), ("diger", 1)]);
        let hidden = HashSet::from(["ayrildigim".to_string()]);
        // Açık kanal ("genel") okunmuş sayılır; ayrıldığım kanal hiç sayılmaz.
        assert_eq!(channel_unread(&cats, &hidden, Some("genel")), 1);
        assert_eq!(channel_unread(&cats, &hidden, None), 5);
    }

    #[test]
    fn sohbet_okunmamislari_kapsamli_ve_gizliler_haric() {
        let list = vec![
            conversation("c1", "u1", Some("A"), 2, false, false),
            // Listeden kaldırılan sohbet rozete sayılmaz.
            conversation("c2", "u2", Some("A"), 9, true, false),
            // Engellediğim kişi.
            conversation("c3", "u3", Some("A"), 4, false, true),
            // Başka proje ve Ana sayfa.
            conversation("c4", "u1", Some("B"), 3, false, false),
            conversation("c5", "u1", None, 6, false, false),
            // Arkadaş listesinden bilinen engel (sohbet satırında bayrak yok).
            conversation("c6", "u6", Some("A"), 5, false, false),
        ];
        let blocked = HashSet::from(["u6".to_string()]);
        assert_eq!(conversation_unread(&list, "A", &blocked, None), 2);
        assert_eq!(conversation_unread(&list, "A", &blocked, Some("c1")), 0);
        assert_eq!(conversation_unread(&list, "B", &blocked, None), 3);
    }
}
