// Standard library imports.
use std::{env, error::Error, fs, process, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, thread, time::Duration};

// External crate imports.
use ctrlc;
use eframe::egui;
#[cfg(unix)]
use nix::sys::signal::{signal, SigHandler, Signal};
use once_cell::sync::Lazy;
use xcap::Window;

// Modules.
mod encounter;
use encounter::{
    encounter_process, get_current_working_dir, load_state, save_state, EncounterState, APP_NAME,
    APP_STATE, STATE_IDLE, STATE_ONGOING, STATE_PAUSE, STATE_QUITTING,
};

// Crate declarations
extern crate image;
extern crate regex;
extern crate rten_imageio;
extern crate rten_tensor;

// Global shutdown flag.
static SHUTDOWN_FLAG: Lazy<Arc<AtomicBool>> =
    Lazy::new(|| Arc::new(AtomicBool::new(false)));

// Global app instance.
static APP_INSTANCE: Lazy<Arc<Mutex<Option<App>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

fn init_engine() -> Result<ocrs::OcrEngine, Box<dyn Error>> {
    let (detection_path, recognition_path) = get_path_to_models();
    let (detection_model, recognition_model) = load_rten_model(detection_path, recognition_path)?;
    create_engine(detection_model, recognition_model)
}

fn create_engine(
    detection_model: rten::Model,
    recognition_model: rten::Model,
) -> Result<ocrs::OcrEngine, Box<dyn Error>> {
    let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;
    Ok(engine)
}

fn load_rten_model(
    detection_path: String,
    recognition_path: String,
) -> Result<(rten::Model, rten::Model), Box<dyn Error>> {
    let detection_model_data = fs::read(detection_path)?;
    let rec_model_data = fs::read(recognition_path)?;
    let detection_model = rten::Model::load(detection_model_data)?;
    let recognition_model = rten::Model::load(rec_model_data)?;
    Ok((detection_model, recognition_model))
}

fn get_path_to_models() -> (String, String) {
    let (exe_path, path) = get_current_working_dir();
    let detection_path = format!("{}/text-detection.rten", path);
    let detection_path_exe = format!("{}/text-detection.rten", exe_path);
    let recognition_path = format!("{}/text-recognition.rten", path);
    let recognition_path_exe = format!("{}/text-recognition.rten", exe_path);
    match fs::read(&detection_path) {
        Ok(_) => (detection_path, recognition_path),
        _ => (detection_path_exe, recognition_path_exe),
    }
}

fn debug_mode() -> Option<Result<(), Box<dyn Error>>> {
    let (exe_path, path) = get_current_working_dir();
    println!("The current directory is {path} exe path {exe_path}");
    for window in Window::all().unwrap().iter() {
        println!("Window: {:?}", (window.app_name(), window.title()));
        if window.title().to_lowercase() == APP_NAME || window.app_name() == APP_NAME {
            let img = window.capture_image().unwrap();
            let _ = img.save("debug.png");
        }
    }
    Some(Ok(()))
}

pub struct App {
    pub encounter_state: Arc<Mutex<EncounterState>>,
    engine: Arc<ocrs::OcrEngine>,
    last_progress: EncounterState,       // Holds initial progress from state.json
    last_rendered_state: EncounterState, // Used for later live updates
    worker_thread: Option<std::thread::JoinHandle<()>>, // Background worker thread
    worker_rx: Option<std::sync::mpsc::Receiver<EncounterState>>, // Message receiver from worker
}

impl App {
    pub fn new() -> Self {
        let engine = Arc::new(init_engine().unwrap());
        let state = load_state().unwrap_or_default();
        let encounter_state = Arc::new(Mutex::new(state));
        let last_progress = encounter_state.lock().unwrap().clone();
        let last_rendered_state = last_progress.clone();
        APP_STATE.store(STATE_IDLE, Ordering::SeqCst);
        Self {
            encounter_state,
            engine,
            last_progress,
            last_rendered_state,
            worker_thread: None,
            worker_rx: None,
        }
    }
    
    fn start_worker(&mut self) {
        if self.worker_thread.is_none() {
            let encounter_state_clone = Arc::clone(&self.encounter_state);
            let engine_clone = Arc::clone(&self.engine);
            let (state_tx, state_rx) = std::sync::mpsc::channel();
            self.worker_rx = Some(state_rx);
    
            self.worker_thread = Some(std::thread::spawn(move || {
                // Use a dynamic sleep: longer sleep when an encounter is active, shorter when idle.
                let mut sleep_duration = 50;
                while APP_STATE.load(Ordering::SeqCst) == STATE_ONGOING {
                    if let Some(window) = Window::all()
                        .ok()
                        .and_then(|w| w.into_iter().find(|w| encounter::game_exist(w)))
                    {
                        if let Ok(mut state) = encounter_state_clone.lock() {
                            // Operate directly on the shared state.
                            let encounter_happened =
                                encounter_process(&engine_clone, &mut *state, &window)
                                    .unwrap_or(false);
                            if encounter_happened {
                                let _ = state_tx.send(state.clone());
                                sleep_duration = 100; // Slow down during an active encounter.
                            } else {
                                sleep_duration = 10; // Poll more frequently when idle.
                            }
                        }
                    } else {
                        sleep_duration = 50;
                    }
                    std::thread::sleep(Duration::from_millis(sleep_duration));
                }
                println!("[DEBUG] Worker thread exiting.");
            }));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if a shutdown has been signaled.
        if SHUTDOWN_FLAG.load(Ordering::SeqCst) {
            eprintln!("Shutdown flag set. Saving final state and exiting...");
            if let Ok(state) = self.encounter_state.lock() {
                save_state(&state, false).unwrap_or_default();
            }
            process::exit(0);
        }
    
        // Start the worker thread if in Ongoing state.
        if APP_STATE.load(Ordering::SeqCst) == STATE_ONGOING {
            if self.worker_thread.is_none() {
                self.start_worker();
            }
        }
    
        // Process state updates from the worker thread.
        if let Some(rx) = &self.worker_rx {
            if let Ok(new_state) = rx.recv_timeout(Duration::from_millis(20)) {
                println!("[DEBUG] UI received new state update!");
                self.last_rendered_state = new_state;
                ctx.request_repaint();
            } else {
                ctx.request_repaint();
            }
        }
    
        let state_copy = if APP_STATE.load(Ordering::SeqCst) == STATE_IDLE {
            self.last_progress.clone()
        } else {
            self.last_rendered_state.clone()
        };
    
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Encounter Counter");
            let state_text = match APP_STATE.load(Ordering::SeqCst) {
                STATE_IDLE => "Idle",
                STATE_ONGOING => "Ongoing",
                STATE_PAUSE => "Paused",
                STATE_QUITTING => "Quitting",
                _ => "Unknown",
            };
            ui.label(format!("App State: {}", state_text));
    
            ui.horizontal(|ui| {
                if ui.button("Start (S)").clicked() {
                    APP_STATE.store(STATE_ONGOING, Ordering::SeqCst);
                }
    
                if ui.button("Pause (P)").clicked() {
                    APP_STATE.store(STATE_PAUSE, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        handle.join().ok();
                    }
                    self.worker_rx = None;
                    {
                        let state_lock = self.encounter_state.lock().unwrap();
                        save_state(&state_lock, false).unwrap_or_default();
                    }
                    ctx.request_repaint();
                }
    
                if ui.button("Reset (R)").clicked() {
                    APP_STATE.store(STATE_IDLE, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        handle.join().ok();
                    }
                    self.worker_rx = None;
                    let new_state = EncounterState::default();
                    {
                        let mut state_lock = self.encounter_state.lock().unwrap();
                        *state_lock = new_state.clone();
                    }
                    {
                        let state_lock = self.encounter_state.lock().unwrap();
                        save_state(&state_lock, false).unwrap_or_default();
                    }
                    self.last_rendered_state = new_state.clone();
                    self.last_progress = new_state;
                    ctx.request_repaint();
                }
    
                if ui.button("Quit (Q)").clicked() {
                    APP_STATE.store(STATE_QUITTING, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        handle.join().ok();
                    }
                    self.worker_rx = None;
                    {
                        let state_lock = self.encounter_state.lock().unwrap();
                        save_state(&state_lock, false).unwrap_or_default();
                    }
                    process::exit(0);
                }
            });
    
            ui.separator();
            ui.label(format!("Total Encounters: {}", state_copy.encounters));
            ui.label(format!("Last Encounters: {}", state_copy.last_encounter.join(", ")));
            ui.separator();
    
            ui.heading("Top 8 Encounters");
            let mut top_encounters: Vec<(&String, &u32)> = state_copy.mon_stats.iter().collect();
            top_encounters.sort_by(|a, b| b.1.cmp(a.1));
            for (i, (mon, count)) in top_encounters.iter().take(8).enumerate() {
                ui.label(format!("{}. {} - {}", i + 1, mon, count));
            }
        });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let state_clone = Arc::clone(&self.encounter_state);
        let save_thread = std::thread::spawn(move || {
            if let Ok(state) = state_clone.lock() {
                if state.unsaved_encounters > 0 {
                    println!("[DEBUG] Saving unsaved encounters before exit...");
                    save_state(&state, false).unwrap_or_default();
                }
            }
        });
        let _ = save_thread.join();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let is_debug = env::args().find(|arg| arg == "debug");
    if is_debug.is_some() {
        if let Some(value) = debug_mode() {
            return value;
        }
    }
    let app = App::new();
    *APP_INSTANCE.lock().unwrap() = Some(app); // Store the app instance globally
    
    // Spawn a thread to monitor the shutdown flag.
    {
        let shutdown_flag = Arc::clone(&SHUTDOWN_FLAG);
        std::thread::spawn(move || {
            while !shutdown_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(100));
            }
            eprintln!("Shutdown flag detected. Exiting application.");
            process::exit(0);
        });
    }
    
    // Handle Ctrl+C (SIGINT) with graceful shutdown.
    ctrlc::set_handler({
        let shutdown_flag = Arc::clone(&SHUTDOWN_FLAG);
        move || {
            eprintln!("Received Ctrl+C! Signaling shutdown...");
            shutdown_flag.store(true, Ordering::SeqCst);
        }
    })
    .expect("Failed to set Ctrl+C handler");
    
    // Handle SIGTERM (Linux/macOS) with graceful shutdown similar to Ctrl+C.
    #[cfg(unix)]
    unsafe {
        extern "C" fn handle_sigterm(_: i32) {
            eprintln!("Received SIGTERM! Signaling shutdown...");
            SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
            // Brief delay to help with final I/O.
            thread::sleep(Duration::from_millis(100));
            process::exit(0);
        }
        signal(Signal::SIGTERM, SigHandler::Handler(handle_sigterm))
            .expect("Failed to set SIGTERM handler");
    }
    
    if let Some(_window) = Window::all()
        .ok()
        .and_then(|w| w.into_iter().find(|w| encounter::game_exist(w)))
    {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([300.0, 350.0]),
            ..Default::default()
        };
        std::panic::set_hook(Box::new(|info| {
            eprintln!("[ERROR] Unexpected crash: {:?}", info);
            let app_guard = APP_INSTANCE.lock().unwrap();
            if let Some(ref app) = *app_guard {
                if let Ok(state) = app.encounter_state.lock() {
                    save_state(&state, false).unwrap_or_default();
                    eprintln!("[ERROR] Saved progress before crash.");
                }
            }
        }));
        eframe::run_native(
            "Encounter Counter",
            native_options,
            Box::new(|_cc| Ok(Box::new(APP_INSTANCE.lock().unwrap().take().unwrap()))),
        )?;
    
        // After run_native returns, perform a final save if shutdown was signaled.
        if SHUTDOWN_FLAG.load(Ordering::SeqCst) {
            eprintln!("Shutdown signal detected. Performing final save...");
            if let Some(app) = APP_INSTANCE.lock().unwrap().take() {
                if let Ok(state) = app.encounter_state.lock() {
                    save_state(&state, false).unwrap_or_default();
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    
        Ok(())
    } else {
        eprintln!("{} game not found", APP_NAME);
        process::exit(1);
    }
}
