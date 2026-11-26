//! Chaîne capture → encodage → envoi en deux étages : la capture (copie et conversion de l'image)
//! se fait pendant que l'image précédente est encodée. Une seule image attend entre les deux :
//! si l'encodeur est en retard, l'image en attente est remplacée par la plus récente (jamais de
//! retard accumulé).
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub struct Latest<T> {
    slot: Mutex<(Option<T>, bool)>,
    cv: Condvar,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            slot: Mutex::new((None, false)),
            cv: Condvar::new(),
        }
    }
}

impl<T> Latest<T> {
    /// Dépose une valeur ; renvoie l'ancienne si elle n'a pas été prise (pour réutiliser
    /// son tampon).
    pub fn put(&self, v: T) -> Option<T> {
        let mut g = self.slot.lock().unwrap();
        let old = g.0.replace(v);
        drop(g);
        self.cv.notify_one();
        old
    }

    /// Prend la valeur la plus récente (attend au plus `timeout`). `None` si fermé ou rien.
    pub fn take(&self, timeout: Duration) -> Option<T> {
        let g = self.slot.lock().unwrap();
        let (mut g, _) = self
            .cv
            .wait_timeout_while(g, timeout, |s| s.0.is_none() && !s.1)
            .unwrap();
        g.0.take()
    }

    pub fn close(&self) {
        self.slot.lock().unwrap().1 = true;
        self.cv.notify_all();
    }

    pub fn closed(&self) -> bool {
        self.slot.lock().unwrap().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn keeps_only_the_latest() {
        let l = Latest::default();
        assert_eq!(l.put(1), None);
        assert_eq!(l.put(2), Some(1));
        assert_eq!(l.take(Duration::from_millis(10)), Some(2));
        assert_eq!(l.take(Duration::from_millis(10)), None);
    }

    #[test]
    fn take_wakes_on_put_and_close() {
        let l = Arc::new(Latest::default());
        let l2 = l.clone();
        let t = std::thread::spawn(move || l2.take(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(30));
        l.put(7);
        assert_eq!(t.join().unwrap(), Some(7));
        let l3 = l.clone();
        let t = std::thread::spawn(move || l3.take(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(30));
        l.close();
        assert_eq!(t.join().unwrap(), None);
        assert!(l.closed());
    }
}
