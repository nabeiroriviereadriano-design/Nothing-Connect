//! Gestion des transferts : offre, acceptation, envoi/réception en threads dédiés,
//! pause/reprise/annulation, reprise après déconnexion, vérification SHA-256.
//!
//! Les E/S disque tournent dans des threads système (pas dans le runtime async) ; les
//! files bornées entre réseau et disque assurent la contre-pression dans les deux sens.

use crate::events::{Event, TransferFileView, TransferState};
use crate::node::Inner;
use crate::speed::SpeedMeter;
use crate::{CoreError, Result};
use nc_protocol::{encode_data_header, DataChunk, Frame, Message, OfferedFile, RejectReason, MAX_CHUNK, MAX_FRAME_PLAINTEXT};
use nc_storage::{now_ms, TransferFileRecord, TransferRecord};
use nc_transfer::{FileOutcome, IncomingFile, OutgoingFile, SourceFile, TransferError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;

const RX_QUEUE: usize = 32;

pub(crate) enum RxCmd {
    Chunk(DataChunk),
    End { idx: u32, sha256: String },
    Pause,
    Stop { discard: bool },
}

pub(crate) struct TransferCtl {
    pub id: Uuid,
    pub peer_id: String,
    pub outgoing: bool,
    pub files: Vec<OfferedFile>,
    sources: Vec<SourceFile>,
    dest: PathBuf,
    done: Vec<AtomicU64>,
    verified: Mutex<Vec<bool>>,
    state: Mutex<TransferState>,
    gen: AtomicU64,
    sender_lock: Mutex<()>,
    rx: Mutex<Option<mpsc::Sender<RxCmd>>>,
    meter: Mutex<SpeedMeter>,
    total: u64,
    current: Mutex<Option<String>>,
    resync: AtomicBool,
}

impl TransferCtl {
    fn new(id: Uuid, peer_id: &str, outgoing: bool, files: Vec<OfferedFile>, sources: Vec<SourceFile>, dest: PathBuf) -> Self {
        let n = files.len();
        let total = files.iter().map(|f| f.size).sum();
        Self {
            id,
            peer_id: peer_id.into(),
            outgoing,
            files,
            sources,
            dest,
            done: (0..n).map(|_| AtomicU64::new(0)).collect(),
            verified: Mutex::new(vec![false; n]),
            state: Mutex::new(TransferState::Offered),
            gen: AtomicU64::new(0),
            sender_lock: Mutex::new(()),
            rx: Mutex::new(None),
            meter: Mutex::new(SpeedMeter::new(0)),
            total,
            current: Mutex::new(None),
            resync: AtomicBool::new(false),
        }
    }

    pub fn state(&self) -> TransferState {
        *self.state.lock().unwrap()
    }

    fn bytes_done(&self) -> u64 {
        self.done.iter().map(|d| d.load(Ordering::Relaxed)).sum()
    }

    fn offsets(&self) -> Vec<u64> {
        self.done.iter().map(|d| d.load(Ordering::Relaxed)).collect()
    }

    fn all_verified(&self) -> bool {
        self.verified.lock().unwrap().iter().all(|v| *v)
    }

    fn stop_sender(&self) {
        self.gen.fetch_add(1, Ordering::SeqCst);
    }

    fn rx_send_blocking_free(&self, cmd: RxCmd) {
        if let Some(tx) = self.rx.lock().unwrap().as_ref() {
            let _ = tx.try_send(cmd);
        }
    }
}

// ---------------------------------------------------------------- utilitaires

fn set_state(inner: &Inner, ctl: &TransferCtl, st: TransferState) {
    *ctl.state.lock().unwrap() = st;
    let _ = inner.store.set_transfer_state(&ctl.id, st.as_str(), st.is_final());
    emit_progress(inner, ctl, true);
}

fn emit_progress(inner: &Inner, ctl: &TransferCtl, force: bool) {
    let done = ctl.bytes_done();
    let (bps, eta, emit) = {
        let mut m = ctl.meter.lock().unwrap();
        let e = m.update(done);
        (m.bps(), m.eta(ctl.total.saturating_sub(done)), e)
    };
    if !(emit || force) {
        return;
    }
    // Aucun verrou ne doit être tenu pendant l'émission (le récepteur peut être lent).
    let current_file = ctl.current.lock().unwrap().clone();
    let state = ctl.state();
    inner.emit(Event::TransferProgress {
        id: ctl.id,
        outgoing: ctl.outgoing,
        state,
        bytes_done: done,
        total_bytes: ctl.total,
        speed_bps: bps,
        eta_s: eta,
        current_file,
    });
}

fn finish(inner: &Inner, ctl: &TransferCtl, st: TransferState, error: Option<String>) {
    ctl.stop_sender();
    *ctl.state.lock().unwrap() = st;
    let _ = inner.store.set_transfer_state(&ctl.id, st.as_str(), true);
    inner.transfers.lock().unwrap().remove(&ctl.id);
    inner.emit(Event::TransferFinished { id: ctl.id, outgoing: ctl.outgoing, state: st, error });
}

fn get(inner: &Inner, id: &Uuid) -> Option<Arc<TransferCtl>> {
    inner.transfers.lock().unwrap().get(id).cloned()
}

fn send_ctrl(inner: &Inner, peer: &str, m: Message) -> bool {
    inner.session(peer).map(|s| s.send(&m)).unwrap_or(false)
}

fn record(ctl: &TransferCtl, state: TransferState) -> TransferRecord {
    TransferRecord {
        id: ctl.id,
        device_id: ctl.peer_id.clone(),
        outgoing: ctl.outgoing,
        created_at: now_ms(),
        finished_at: None,
        state: state.as_str().into(),
        total_bytes: ctl.total,
        dest_dir: (!ctl.outgoing).then(|| ctl.dest.to_string_lossy().into_owned()),
        files: ctl
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| TransferFileRecord {
                idx: i as u32,
                rel_path: f.rel_path.clone(),
                size: f.size,
                source_path: ctl.sources.get(i).map(|s| s.path.to_string_lossy().into_owned()),
                final_path: None,
                sha256: None,
                state: "pending".into(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- envoi

pub(crate) fn send_paths(inner: &Arc<Inner>, _rt: &tokio::runtime::Handle, peer_id: &str, paths: Vec<PathBuf>) -> Result<Uuid> {
    let session = inner.session(peer_id).ok_or(CoreError::NotConnected)?;
    let sources = nc_transfer::plan(&paths)?;
    if sources.is_empty() {
        return Err(CoreError::InvalidState);
    }
    let files: Vec<OfferedFile> = sources.iter().map(|s| s.offered()).collect();
    let id = Uuid::new_v4();
    let ctl = Arc::new(TransferCtl::new(id, peer_id, true, files.clone(), sources, PathBuf::new()));
    inner.store.insert_transfer(&record(&ctl, TransferState::Offered))?;
    inner.transfers.lock().unwrap().insert(id, ctl.clone());
    if !session.send(&Message::TransferOffer { id, files }) {
        set_state(inner, &ctl, TransferState::Interrupted);
        return Err(CoreError::NotConnected);
    }
    emit_progress(inner, &ctl, true);
    Ok(id)
}

fn start_sender(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>, offsets: Vec<u64>) {
    let gen = ctl.gen.fetch_add(1, Ordering::SeqCst) + 1;
    for (d, o) in ctl.done.iter().zip(&offsets) {
        d.store(*o, Ordering::Relaxed);
    }
    ctl.meter.lock().unwrap().reset(ctl.bytes_done());
    set_state(inner, ctl, TransferState::Running);
    let (inner, ctl) = (inner.clone(), ctl.clone());
    std::thread::Builder::new()
        .name("nc-send".into())
        .spawn(move || {
            if let Err(e) = run_sender(&inner, &ctl, gen, &offsets) {
                if ctl.gen.load(Ordering::SeqCst) == gen {
                    send_ctrl(&inner, &ctl.peer_id, Message::TransferCancel { id: ctl.id });
                    finish(&inner, &ctl, TransferState::Failed, Some(e.to_string()));
                }
            }
        })
        .expect("thread d'envoi");
}

fn run_sender(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>, gen: u64, offsets: &[u64]) -> std::result::Result<(), TransferError> {
    // Une seule génération d'envoi à la fois (l'ancienne termine son bloc en cours).
    let _guard = ctl.sender_lock.lock().unwrap();
    let alive = || ctl.gen.load(Ordering::SeqCst) == gen;
    let Some(session) = inner.session(&ctl.peer_id) else { return Ok(()) };
    let data = session.data_sender();
    for (i, src) in ctl.sources.iter().enumerate() {
        if !alive() {
            return Ok(());
        }
        if ctl.verified.lock().unwrap()[i] {
            continue;
        }
        *ctl.current.lock().unwrap() = Some(src.rel_path.clone());
        let mut f = OutgoingFile::open(src, offsets.get(i).copied().unwrap_or(0))?;
        loop {
            if !alive() {
                return Ok(());
            }
            let mut buf = Vec::with_capacity(MAX_FRAME_PLAINTEXT);
            encode_data_header(&mut buf, &ctl.id, i as u32, f.position());
            if f.next_chunk(&mut buf, MAX_CHUNK)?.is_none() {
                break;
            }
            if data.blocking_send(buf).is_err() {
                return Ok(()); // session fermée : état « interrompu » géré par la session
            }
            ctl.done[i].store(f.position(), Ordering::Relaxed);
            emit_progress(inner, ctl, false);
        }
        let sha256 = f.finish();
        let end = Frame::Control(Message::TransferFileEnd { id: ctl.id, file_index: i as u32, sha256 }).encode().expect("petit message");
        if data.blocking_send(end).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

pub(crate) fn on_accept(inner: &Arc<Inner>, peer: &str, id: Uuid, offsets: Vec<u64>) {
    let Some(ctl) = get(inner, &id) else { return };
    if !ctl.outgoing || ctl.peer_id != peer || offsets.len() != ctl.files.len() {
        return;
    }
    let offsets = offsets.iter().zip(&ctl.files).map(|(o, f)| (*o).min(f.size)).collect();
    start_sender(inner, &ctl, offsets);
}

pub(crate) fn on_reject(inner: &Arc<Inner>, id: Uuid) {
    if let Some(ctl) = get(inner, &id) {
        if ctl.outgoing {
            finish(inner, &ctl, TransferState::Rejected, Some("refusé par le destinataire".into()));
        }
    }
}

pub(crate) fn on_verified(inner: &Arc<Inner>, id: Uuid, idx: u32, ok: bool) {
    let Some(ctl) = get(inner, &id) else { return };
    if !ctl.outgoing || idx as usize >= ctl.files.len() {
        return;
    }
    let _ = inner.store.set_file_result(&id, idx, if ok { "completed" } else { "failed" }, None, None);
    if !ok {
        finish(inner, &ctl, TransferState::Failed, Some(format!("{} : empreinte différente à la réception", ctl.files[idx as usize].rel_path)));
        return;
    }
    ctl.verified.lock().unwrap()[idx as usize] = true;
    ctl.done[idx as usize].store(ctl.files[idx as usize].size, Ordering::Relaxed);
    inner.emit(Event::TransferFileDone { id, index: idx, path: None, outcome: "sent" });
    if ctl.all_verified() {
        emit_progress(inner, &ctl, true);
        finish(inner, &ctl, TransferState::Completed, None);
    }
}

// ---------------------------------------------------------------- réception

fn validate_offer(files: &[OfferedFile]) -> bool {
    !files.is_empty() && files.len() <= nc_transfer::MAX_FILES && files.iter().all(|f| nc_transfer::safe_relative(&f.rel_path).is_ok())
}

pub(crate) fn on_offer(inner: &Arc<Inner>, peer: &str, id: Uuid, files: Vec<OfferedFile>) {
    if !validate_offer(&files) {
        send_ctrl(inner, peer, Message::TransferReject { id, reason: RejectReason::Invalid });
        return;
    }
    // Reprise d'un transfert connu (même identifiant, même liste) : acceptation automatique.
    if let Some(ctl) = get(inner, &id) {
        if !ctl.outgoing && ctl.peer_id == peer && ctl.files == files {
            if ctl.state() == TransferState::Waiting {
                inner.emit(incoming_event(&ctl));
            } else {
                start_receiving(inner, &ctl);
            }
            return;
        }
        send_ctrl(inner, peer, Message::TransferReject { id, reason: RejectReason::Invalid });
        return;
    }
    if let Ok(Some(rec)) = inner.store.transfer(&id) {
        if !rec.outgoing && rec.device_id == peer && !matches!(rec.state.as_str(), "completed" | "cancelled" | "rejected" | "failed") {
            let dest = rec.dest_dir.map(PathBuf::from).unwrap_or_else(|| inner.download_dir());
            let ctl = Arc::new(TransferCtl::new(id, peer, false, files, vec![], dest));
            for f in rec.files.iter().filter(|f| matches!(f.state.as_str(), "completed" | "already_present" | "skipped")) {
                if let Some(v) = ctl.verified.lock().unwrap().get_mut(f.idx as usize) {
                    *v = true;
                }
            }
            inner.transfers.lock().unwrap().insert(id, ctl.clone());
            start_receiving(inner, &ctl);
            return;
        }
    }
    let ctl = Arc::new(TransferCtl::new(id, peer, false, files, vec![], inner.download_dir()));
    if inner.store.insert_transfer(&record(&ctl, TransferState::Waiting)).is_err() {
        send_ctrl(inner, peer, Message::TransferReject { id, reason: RejectReason::Busy });
        return;
    }
    inner.transfers.lock().unwrap().insert(id, ctl.clone());
    let auto = inner.store.device(peer).ok().flatten().map(|d| d.auto_accept).unwrap_or(false);
    if auto {
        start_receiving(inner, &ctl);
    } else {
        *ctl.state.lock().unwrap() = TransferState::Waiting;
        inner.emit(incoming_event(&ctl));
    }
}

fn incoming_event(ctl: &TransferCtl) -> Event {
    Event::TransferIncoming {
        id: ctl.id,
        peer_id: ctl.peer_id.clone(),
        files: ctl.files.iter().map(|f| TransferFileView { rel_path: f.rel_path.clone(), size: f.size }).collect(),
        total_bytes: ctl.total,
    }
}

pub(crate) fn answer_offer(inner: &Arc<Inner>, id: &Uuid, accept: bool) -> Result<()> {
    let ctl = get(inner, id).ok_or(CoreError::UnknownTransfer)?;
    if ctl.outgoing || ctl.state() != TransferState::Waiting {
        return Err(CoreError::InvalidState);
    }
    if accept {
        if inner.session(&ctl.peer_id).is_none() {
            return Err(CoreError::NotConnected);
        }
        start_receiving(inner, &ctl);
    } else {
        send_ctrl(inner, &ctl.peer_id, Message::TransferReject { id: *id, reason: RejectReason::UserDeclined });
        finish(inner, &ctl, TransferState::Rejected, None);
    }
    Ok(())
}

fn start_receiving(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>) {
    // Arrête un éventuel thread précédent (reconnexion) en conservant les partiels.
    if let Some(old) = ctl.rx.lock().unwrap().take() {
        let _ = old.try_send(RxCmd::Stop { discard: false });
    }
    let mut offsets = nc_transfer::resume_offsets(&ctl.dest, &ctl.id, &ctl.files);
    {
        let v = ctl.verified.lock().unwrap();
        for (i, o) in offsets.iter_mut().enumerate() {
            if v[i] {
                *o = ctl.files[i].size;
            }
        }
    }
    for (d, o) in ctl.done.iter().zip(&offsets) {
        d.store(*o, Ordering::Relaxed);
    }
    let (tx, rx) = mpsc::channel(RX_QUEUE);
    *ctl.rx.lock().unwrap() = Some(tx);
    ctl.resync.store(false, Ordering::SeqCst);
    ctl.meter.lock().unwrap().reset(ctl.bytes_done());
    set_state(inner, ctl, TransferState::Running);
    let (i2, c2) = (inner.clone(), ctl.clone());
    std::thread::Builder::new().name("nc-recv".into()).spawn(move || run_receiver(&i2, &c2, rx)).expect("thread de réception");
    send_ctrl(inner, &ctl.peer_id, Message::TransferAccept { id: ctl.id, offsets });
}

pub(crate) async fn on_chunk(inner: &Arc<Inner>, chunk: DataChunk) {
    let Some(ctl) = get(inner, &chunk.transfer) else { return };
    let tx = ctl.rx.lock().unwrap().clone();
    if let Some(tx) = tx {
        let _ = tx.send(RxCmd::Chunk(chunk)).await; // contre-pression : attend le disque
    }
}

pub(crate) async fn on_file_end(inner: &Arc<Inner>, id: Uuid, idx: u32, sha256: String) {
    let Some(ctl) = get(inner, &id) else { return };
    let tx = ctl.rx.lock().unwrap().clone();
    if let Some(tx) = tx {
        let _ = tx.send(RxCmd::End { idx, sha256 }).await;
    }
}

fn run_receiver(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>, mut rx: mpsc::Receiver<RxCmd>) {
    let policy = inner.settings().conflict_policy;
    let mut current: Option<(u32, IncomingFile)> = None;

    let request_resync = |current: &Option<(u32, IncomingFile)>| {
        if !ctl.resync.swap(true, Ordering::SeqCst) {
            if let Some((i, f)) = current {
                ctl.done[*i as usize].store(f.position(), Ordering::Relaxed);
            }
            send_ctrl(inner, &ctl.peer_id, Message::TransferResume { id: ctl.id, offsets: ctl.offsets() });
        }
    };

    let fail = |e: String| {
        send_ctrl(inner, &ctl.peer_id, Message::TransferCancel { id: ctl.id });
        finish(inner, ctl, TransferState::Failed, Some(e));
    };

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            RxCmd::Chunk(c) => {
                let idx = c.file_index;
                if idx as usize >= ctl.files.len() || ctl.verified.lock().unwrap()[idx as usize] {
                    continue;
                }
                if current.as_ref().map(|(i, _)| *i) != Some(idx) {
                    if let Some((_, mut f)) = current.take() {
                        let _ = f.sync();
                    }
                    match IncomingFile::open(&ctl.dest, &ctl.id, idx, &ctl.files[idx as usize], ctl.done[idx as usize].load(Ordering::Relaxed)) {
                        Ok(f) => {
                            *ctl.current.lock().unwrap() = Some(ctl.files[idx as usize].rel_path.clone());
                            current = Some((idx, f));
                        }
                        Err(e) => return fail(e.to_string()),
                    }
                }
                let (_, f) = current.as_mut().unwrap();
                let pos = f.position();
                let end = c.offset + c.data.len() as u64;
                if end <= pos {
                    continue; // déjà reçu (blocs en vol avant une reprise)
                }
                if c.offset > pos {
                    request_resync(&current);
                    continue;
                }
                ctl.resync.store(false, Ordering::SeqCst);
                let skip = (pos - c.offset) as usize;
                if let Err(e) = f.write_chunk(pos, &c.data[skip..]) {
                    return fail(e.to_string());
                }
                ctl.done[idx as usize].store(f.position(), Ordering::Relaxed);
                emit_progress(inner, ctl, false);
            }
            RxCmd::End { idx, sha256 } => {
                if idx as usize >= ctl.files.len() || ctl.verified.lock().unwrap()[idx as usize] {
                    continue;
                }
                let f = match current.take() {
                    Some((i, f)) if i == idx => f,
                    other => {
                        if let Some((_, mut f)) = other {
                            let _ = f.sync();
                        }
                        // Fichier vide, ou déjà entièrement reçu avant une reprise.
                        match IncomingFile::open(&ctl.dest, &ctl.id, idx, &ctl.files[idx as usize], ctl.done[idx as usize].load(Ordering::Relaxed)) {
                            Ok(f) => f,
                            Err(e) => return fail(e.to_string()),
                        }
                    }
                };
                if f.position() != ctl.files[idx as usize].size {
                    ctl.done[idx as usize].store(f.position(), Ordering::Relaxed);
                    drop(f);
                    request_resync(&None);
                    continue;
                }
                match f.finish(&sha256, policy) {
                    Ok((outcome, digest)) => {
                        let (state, label) = match &outcome {
                            FileOutcome::Saved(_) => ("completed", "saved"),
                            FileOutcome::AlreadyPresent(_) => ("already_present", "already_present"),
                            FileOutcome::Skipped(_) => ("skipped", "skipped"),
                        };
                        let path = outcome.path().to_string_lossy().into_owned();
                        let _ = inner.store.set_file_result(&ctl.id, idx, state, Some(&digest), Some(&path));
                        ctl.verified.lock().unwrap()[idx as usize] = true;
                        ctl.done[idx as usize].store(ctl.files[idx as usize].size, Ordering::Relaxed);
                        send_ctrl(inner, &ctl.peer_id, Message::TransferFileVerified { id: ctl.id, file_index: idx, ok: true });
                        inner.emit(Event::TransferFileDone { id: ctl.id, index: idx, path: Some(path), outcome: label });
                        if ctl.all_verified() {
                            emit_progress(inner, ctl, true);
                            finish(inner, ctl, TransferState::Completed, None);
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = inner.store.set_file_result(&ctl.id, idx, "failed", None, None);
                        send_ctrl(inner, &ctl.peer_id, Message::TransferFileVerified { id: ctl.id, file_index: idx, ok: false });
                        finish(inner, ctl, TransferState::Failed, Some(format!("{} : {e}", ctl.files[idx as usize].rel_path)));
                        return;
                    }
                }
            }
            RxCmd::Pause => {
                if let Some((_, f)) = current.as_mut() {
                    let _ = f.sync();
                }
            }
            RxCmd::Stop { discard } => {
                if discard {
                    if let Some((_, f)) = current.take() {
                        f.discard();
                    }
                    nc_transfer::discard_partials(&ctl.dest, &ctl.id, ctl.files.len());
                } else if let Some((_, mut f)) = current.take() {
                    let _ = f.sync();
                }
                return;
            }
        }
    }
    if let Some((_, mut f)) = current.take() {
        let _ = f.sync();
    }
    if ctl.state() == TransferState::Cancelled {
        nc_transfer::discard_partials(&ctl.dest, &ctl.id, ctl.files.len());
    }
}

// ---------------------------------------------------------------- contrôle utilisateur

pub(crate) fn pause(inner: &Arc<Inner>, id: &Uuid) -> Result<()> {
    let ctl = get(inner, id).ok_or(CoreError::UnknownTransfer)?;
    if ctl.state() != TransferState::Running {
        return Err(CoreError::InvalidState);
    }
    if ctl.outgoing {
        ctl.stop_sender();
    } else {
        ctl.rx_send_blocking_free(RxCmd::Pause);
    }
    send_ctrl(inner, &ctl.peer_id, Message::TransferPause { id: *id });
    set_state(inner, &ctl, TransferState::Paused);
    Ok(())
}

pub(crate) fn resume(inner: &Arc<Inner>, id: &Uuid) -> Result<()> {
    let ctl = get(inner, id).ok_or(CoreError::UnknownTransfer)?;
    let st = ctl.state();
    if inner.session(&ctl.peer_id).is_none() {
        return Err(CoreError::NotConnected);
    }
    match (st, ctl.outgoing) {
        (TransferState::Paused, false) => {
            ctl.resync.store(false, Ordering::SeqCst);
            ctl.meter.lock().unwrap().reset(ctl.bytes_done());
            set_state(inner, &ctl, TransferState::Running);
            send_ctrl(inner, &ctl.peer_id, Message::TransferResume { id: *id, offsets: ctl.offsets() });
        }
        (TransferState::Paused, true) => {
            // Demande au destinataire ses offsets exacts.
            send_ctrl(inner, &ctl.peer_id, Message::TransferResume { id: *id, offsets: vec![] });
        }
        (TransferState::Interrupted, true) => reoffer(inner, &ctl),
        _ => return Err(CoreError::InvalidState),
    }
    Ok(())
}

pub(crate) fn cancel(inner: &Arc<Inner>, id: &Uuid) -> Result<()> {
    let ctl = get(inner, id).ok_or(CoreError::UnknownTransfer)?;
    send_ctrl(inner, &ctl.peer_id, Message::TransferCancel { id: *id });
    local_cancel(inner, &ctl);
    Ok(())
}

fn local_cancel(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>) {
    ctl.stop_sender();
    if !ctl.outgoing {
        let had_thread = ctl.rx.lock().unwrap().is_some();
        ctl.rx_send_blocking_free(RxCmd::Stop { discard: true });
        *ctl.rx.lock().unwrap() = None;
        if !had_thread {
            nc_transfer::discard_partials(&ctl.dest, &ctl.id, ctl.files.len());
        }
    }
    finish(inner, ctl, TransferState::Cancelled, None);
}

pub(crate) fn on_remote_pause(inner: &Arc<Inner>, id: Uuid) {
    let Some(ctl) = get(inner, &id) else { return };
    if ctl.state() != TransferState::Running {
        return;
    }
    if ctl.outgoing {
        ctl.stop_sender();
    } else {
        ctl.rx_send_blocking_free(RxCmd::Pause);
    }
    set_state(inner, &ctl, TransferState::Paused);
}

pub(crate) fn on_remote_resume(inner: &Arc<Inner>, peer: &str, id: Uuid, offsets: Vec<u64>) {
    let Some(ctl) = get(inner, &id) else { return };
    if ctl.peer_id != peer {
        return;
    }
    if ctl.outgoing {
        if offsets.len() == ctl.files.len() {
            let offsets = offsets.iter().zip(&ctl.files).map(|(o, f)| (*o).min(f.size)).collect();
            start_sender(inner, &ctl, offsets);
        }
    } else if offsets.is_empty() && matches!(ctl.state(), TransferState::Paused | TransferState::Running) {
        ctl.resync.store(false, Ordering::SeqCst);
        set_state(inner, &ctl, TransferState::Running);
        send_ctrl(inner, peer, Message::TransferResume { id, offsets: ctl.offsets() });
    }
}

pub(crate) fn on_remote_cancel(inner: &Arc<Inner>, id: Uuid) {
    if let Some(ctl) = get(inner, &id) {
        local_cancel(inner, &ctl);
    }
}

// ---------------------------------------------------------------- connexion / déconnexion

fn reoffer(inner: &Arc<Inner>, ctl: &Arc<TransferCtl>) {
    if send_ctrl(inner, &ctl.peer_id, Message::TransferOffer { id: ctl.id, files: ctl.files.clone() }) {
        set_state(inner, ctl, TransferState::Offered);
    }
}

pub(crate) fn on_connected(inner: &Arc<Inner>, peer: &str) {
    // Transferts sortants interrompus encore en mémoire.
    let mem: Vec<_> = inner
        .transfers
        .lock()
        .unwrap()
        .values()
        .filter(|c| c.outgoing && c.peer_id == peer && c.state() == TransferState::Interrupted)
        .cloned()
        .collect();
    for c in &mem {
        reoffer(inner, c);
    }
    // Transferts interrompus avant un redémarrage de l'application.
    let Ok(recs) = inner.store.resumable_outgoing(peer) else { return };
    for rec in recs.into_iter().filter(|r| r.state == "interrupted") {
        if inner.transfers.lock().unwrap().contains_key(&rec.id) {
            continue;
        }
        let paths: Option<Vec<PathBuf>> = rec.files.iter().map(|f| f.source_path.as_ref().map(PathBuf::from)).collect();
        let Some(paths) = paths else { continue };
        let mut sources = Vec::new();
        let mut ok = true;
        for (p, f) in paths.iter().zip(&rec.files) {
            match std::fs::metadata(p) {
                Ok(m) if m.len() == f.size => sources.push(SourceFile { path: p.clone(), rel_path: f.rel_path.clone(), size: f.size, modified_ms: None }),
                _ => ok = false,
            }
        }
        if !ok {
            let _ = inner.store.set_transfer_state(&rec.id, "failed", true);
            continue;
        }
        let files: Vec<OfferedFile> = sources.iter().map(|s| s.offered()).collect();
        let ctl = Arc::new(TransferCtl::new(rec.id, peer, true, files, sources, PathBuf::new()));
        for f in rec.files.iter().filter(|f| f.state == "completed") {
            ctl.verified.lock().unwrap()[f.idx as usize] = true;
        }
        inner.transfers.lock().unwrap().insert(rec.id, ctl.clone());
        reoffer(inner, &ctl);
    }
}

pub(crate) fn on_disconnected(inner: &Arc<Inner>, peer: &str) {
    let list: Vec<_> = inner.transfers.lock().unwrap().values().filter(|c| c.peer_id == peer).cloned().collect();
    for ctl in list {
        if ctl.state().is_final() {
            continue;
        }
        ctl.stop_sender();
        if !ctl.outgoing {
            ctl.rx_send_blocking_free(RxCmd::Stop { discard: false });
            *ctl.rx.lock().unwrap() = None;
        }
        set_state(inner, &ctl, TransferState::Interrupted);
    }
}

/// Annule silencieusement tous les transferts d'un appareil (révocation).
pub(crate) fn abort_peer(inner: &Arc<Inner>, peer: &str) {
    let list: Vec<_> = inner.transfers.lock().unwrap().values().filter(|c| c.peer_id == peer).cloned().collect();
    for ctl in list {
        local_cancel(inner, &ctl);
    }
}
