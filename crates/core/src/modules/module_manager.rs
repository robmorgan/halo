use std::collections::HashMap;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::traits::{AsyncModule, ModuleEvent, ModuleId, ModuleMessage};

pub struct ModuleManager {
    modules: HashMap<ModuleId, Box<dyn AsyncModule>>,
    module_handles: HashMap<ModuleId, JoinHandle<()>>,
    module_senders: HashMap<ModuleId, mpsc::Sender<ModuleEvent>>,
    message_receiver: Option<mpsc::Receiver<ModuleMessage>>,
    message_sender: mpsc::Sender<ModuleMessage>,
    running: bool,
}

impl ModuleManager {
    pub fn new() -> Self {
        let (message_sender, message_receiver) = mpsc::channel(1000);

        Self {
            modules: HashMap::new(),
            module_handles: HashMap::new(),
            module_senders: HashMap::new(),
            message_receiver: Some(message_receiver),
            message_sender,
            running: false,
        }
    }

    /// Register a new module with the manager
    pub fn register_module(&mut self, module: Box<dyn AsyncModule>) {
        let id = module.id();
        self.modules.insert(id, module);
    }

    /// Initialize all registered modules
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (id, module) in &mut self.modules {
            match module.initialize().await {
                Ok(_) => log::info!("Module {:?} initialized successfully", id),
                Err(e) => {
                    log::error!("Failed to initialize module {:?}: {}", id, e);
                    let error_message = format!("{:?}Module error: {}", id, e);
                    return Err(error_message.into());
                }
            }
        }
        Ok(())
    }

    /// Start all modules and begin the main coordination loop
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.running {
            return Err("Module manager is already running".into());
        }

        // Start each module in its own async task
        let modules_to_start = std::mem::take(&mut self.modules);

        for (id, mut module) in modules_to_start {
            let (event_tx, event_rx) = mpsc::channel(1000);
            let message_tx = self.message_sender.clone();
            let module_id = id.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = module.run(event_rx, message_tx.clone()).await {
                    let _ = message_tx
                        .send(ModuleMessage::Error(format!(
                            "Module {:?} error: {}",
                            module_id, e
                        )))
                        .await;
                }
            });

            self.module_handles.insert(id.clone(), handle);
            self.module_senders.insert(id, event_tx);
        }

        self.running = true;
        Ok(())
    }

    /// Send an event to a specific module
    pub async fn send_to_module(
        &self,
        module_id: ModuleId,
        event: ModuleEvent,
    ) -> Result<(), String> {
        if let Some(sender) = self.module_senders.get(&module_id) {
            sender
                .send(event)
                .await
                .map_err(|e| format!("Failed to send event to module {:?}: {}", module_id, e))?;
            Ok(())
        } else {
            Err(format!("Module {:?} not found", module_id))
        }
    }

    /// Broadcast an event to all modules
    pub async fn broadcast_event(&self, event: ModuleEvent) {
        for (id, sender) in &self.module_senders {
            if let Err(e) = sender.send(event.clone()).await {
                log::warn!("Failed to broadcast event to module {:?}: {}", id, e);
            }
        }
    }

    /// Get the message receiver (should only be called once)
    pub fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ModuleMessage>> {
        self.message_receiver.take()
    }

    /// Shutdown all modules gracefully
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.running {
            return Ok(());
        }

        log::info!("Shutting down module manager...");

        // Send shutdown event to all modules
        self.broadcast_event(ModuleEvent::Shutdown).await;

        // Wait for all module handles to complete
        for (id, handle) in std::mem::take(&mut self.module_handles) {
            log::info!("Waiting for module {:?} to shutdown...", id);
            if let Err(e) = handle.await {
                log::error!("Module {:?} shutdown error: {}", id, e);
            }
        }

        // Clear the senders map as well
        self.module_senders.clear();

        self.running = false;
        log::info!("Module manager shutdown complete");
        Ok(())
    }

    /// Check if the manager is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get status of all modules
    pub fn get_status(&self) -> HashMap<ModuleId, HashMap<String, String>> {
        let mut status = HashMap::new();
        for (id, module) in &self.modules {
            status.insert(id.clone(), module.status());
        }
        status
    }
}
