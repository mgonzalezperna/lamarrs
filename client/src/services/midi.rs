use std::{
    fmt::{self}, time::Duration,
};

use lamarrs_utils::{MidiInstruction, Service};
use midir::{InitError, MidiOutput, MidiOutputConnection as OutputConnection, PortInfoError, SendError};
use midly::{MidiMessage, num::u7};
use tokio::{sync::mpsc::{self, Receiver, Sender, channel}, time::sleep};
use tracing::info;

use crate::InternalEventMessageClient;

/// Wrapper for MidiOutputConnection to add Debug.
pub struct MidiOutputConnection(pub OutputConnection);

impl fmt::Debug for MidiOutputConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<MidiOutputConnection>")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MidiServiceError {
    #[error("There was a irrecoverable error with the service {}.", service)]
    Service { service: String },
    #[error("Error sending an InternalMessage to Server handler.")]
    SendInternalMessage(#[from] mpsc::error::SendError<InternalEventMessageClient>),
    #[error("Failed creating the Midi Channel.")]
    FailedInitialisingMidiChannel(#[from] InitError),
    #[error("No MIDI output ports available.")]
    NoMidiOutputAvailable(),
    #[error("Failed to recover the MIDI port names.")]
    NoMidiPortNames(#[from] PortInfoError),
    #[error("Failed to Connect to Midi Output channel: {description}")]
    FailedToConnectToMidiOutput{description: String},
    #[error("Output MIDI message could not be deliver.")]
    FailedToSendMidiMessage(#[from] SendError)
}

#[derive(Debug)]
pub struct MidiService {
    pub sender: Sender<InternalEventMessageClient>,
    receiver: Receiver<InternalEventMessageClient>,
    midi_port_output_connection: MidiOutputConnection,
    default_channel: u8, // 1–16
}

impl fmt::Display for MidiService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MidiService")
    }
}

impl MidiService {
    pub fn try_new(output_midi_port_name: String, default_channel: Option<u8>) -> Result<Self, MidiServiceError> {
        let (sender, receiver) = channel(32);
        // MIDI service requires a MidiPort that we can get from the CLI params.
        // We use `midir`, which works only with ALSA, therefore there is only one way to retrieve the port names:
        // `$ aconnect -l`
        // Select the port name from the output. i.e.
        //
        // client 0: 'System' [type=kernel]
        //      0 'Timer           '
        //          Connecting To: 142:0
        //      1 'Announce        '
        //          Connecting To: 142:0
        // client 14: 'Midi Through' [type=kernel]
        //      0 'Midi Through Port-0'
        // client 24: 'CH345' [type=kernel,card=2]
        //      0 'CH345 MIDI 1
        //
        // In this case, port names could be `Midi Through Port-0` or `CH345`.
        // Check that the port is not `Input` only!
        // Create a new MIDI output context
        let midi_out = MidiOutput::new("MidiSender")?;
        // Enumerate all available output ports
        let out_ports = midi_out.ports();
        if out_ports.is_empty() { return Err(MidiServiceError::NoMidiOutputAvailable()) };

        // Try to find one whose name matches
        for port in out_ports {
            let name = midi_out.port_name(&port)?;
            info!("Checking port with name: {}", name);
            if name.contains(&output_midi_port_name) {
                let midi_port_output_connection = midi_out.connect(&port, &format!("midir-conn:{}", name)).map_err(|e| MidiServiceError::FailedToConnectToMidiOutput{description: e.to_string()})?;
                return Ok(Self {
                    sender,
                    receiver,
                    midi_port_output_connection: MidiOutputConnection(midi_port_output_connection),
                    default_channel: default_channel.unwrap_or(1)
                });
            }
        };
        Err(MidiServiceError::Service{service: "Midi service. The selcted `output_midi_port_name` doesn't exist".into()})
    }

    pub async fn receive_message(&mut self) -> Option<InternalEventMessageClient> {
        self.receiver.recv().await
    }

    /// Runs the Service.
    pub async fn run(&mut self) -> Result<(), MidiServiceError> {
        loop {
            while let Some(message) = self.receive_message().await {
                match message {
                    InternalEventMessageClient::ConnectedToServer(sender) => {
                        self.subscribe_to_remote_service(sender).await?
                    }
                    InternalEventMessageClient::NewMIDIMessage(midi_instruction, sender) => {
                        self.preset_change(midi_instruction.new_preset.get()-1).await? // This solves the `offset-by-1` issues with the presets, as the list never start with 0 in the interfaces, but internally they do.
                    }
                    InternalEventMessageClient::Config(_) => {
                        unimplemented!("This message is not yet functional.")
                    }
                    _ => {
                        return Err(MidiServiceError::Service {
                            service: self.to_string(),
                        })
                    }
                }
            }
        }
    }

    /// Each service must know what remote services needs to consume. A Client Service could
    /// several different messages if required.
    async fn subscribe_to_remote_service(
        &mut self,
        server_sender: Sender<InternalEventMessageClient>,
    ) -> Result<(), MidiServiceError> {
        info!("{} subscribing to Midi messages", self.to_string());
        Ok(server_sender
            .send(InternalEventMessageClient::SubscribeToService(
                Service::Midi,
            ))
            .await?)
    }

    /// Convert a midly MidiMessage into bytes and send via midir
    async fn send_message(&mut self, msg: MidiMessage) -> Result<(), MidiServiceError> {
        let status = match msg {
            MidiMessage::NoteOff { .. } => 0x80,
            MidiMessage::NoteOn { .. } => 0x90,
            MidiMessage::Aftertouch { .. } => 0xA0,
            MidiMessage::Controller { .. } => 0xB0,
            MidiMessage::ProgramChange { .. } => 0xC0,
            MidiMessage::ChannelAftertouch { .. } => 0xD0,
            MidiMessage::PitchBend { .. } => 0xE0,
        } | ((self.default_channel - 1) & 0x0F);

        // Serialize message
        let mut bytes = vec![status];
        match msg {
            MidiMessage::ProgramChange { program } => bytes.push(program.as_int() & 0x7F),
            MidiMessage::Controller { controller, value } => {
                bytes.push(controller.as_int() & 0x7F);
                bytes.push(value.as_int() & 0x7F);
            }
            _ => unimplemented!("Only CC and PC are currently supported"),
        }

        Ok(self.midi_port_output_connection.0.send(&bytes)?)
    }

    async fn preset_change(&mut self, preset: u16) -> Result<(), MidiServiceError> {
        let bank = (preset / 128) as u8;
        let program = (preset % 128) as u8;
        self.bank_select(0, bank).await?;
        self.program_change(program).await?;
        Ok(())
    }

    async fn program_change(&mut self, program: u8) -> Result<(), MidiServiceError> {
        let msg = MidiMessage::ProgramChange {
            program: u7::from(program.min(127)),
        };
        self.send_message(msg).await
    }

    async fn control_change(&mut self, cc: u8, value: u8) -> Result<(), MidiServiceError> {
        let msg = MidiMessage::Controller {
            controller: u7::from(cc.min(127)),
            value: u7::from(value.min(127)),
        };
        self.send_message(msg).await
    }

    async fn bank_select(&mut self, msb: u8, lsb: u8) -> Result<(), MidiServiceError> {
        self.control_change(0, msb).await?;
        sleep(Duration::from_millis(10)).await;
        self.control_change(32, lsb).await
    }

    async fn sysex(&mut self, data: &[u8]) -> Result<(), MidiServiceError> {
        Ok(self.midi_port_output_connection.0.send(data)?)
    }
}
