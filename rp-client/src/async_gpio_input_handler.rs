use cyw43::Control;
use embassy_rp::gpio::Input;
use embassy_time::Instant;

use crate::ASYNC_GPIO_INPUT_CHANNEL;

/// Events that worker tasks send to the OLED_CHANNEL.
#[derive(defmt::Format)]
pub enum GpioInputEvents {
    NextTrigger, // Send message to Server to trigger next scene.
    Retrigger,   // Send message to Server to retrigger the current scene.
}

#[embassy_executor::task]
pub async fn async_input_handler(mut async_input: Input<'static>, mut control: Control<'static>) {
    // Create sender for button.
    let button_sender = ASYNC_GPIO_INPUT_CHANNEL.sender();
    defmt::info!("Button handler started. Led off.");
    control.gpio_set(0, true).await;
    loop {
        async_input.wait_for_low().await;
        defmt::debug!("Button interruption received. Led on.");
        control.gpio_set(0, false).await;
        let interruption_starttime = Instant::now();
        async_input.wait_for_high().await;
        let interruption_endtime = Instant::now();
        let duration = interruption_endtime - interruption_starttime; // Button pressed `Duration`
        let duration_ms = duration.as_millis(); // Convert to milliseconds
        defmt::debug!("Processing duration time:{}", duration_ms);
        match duration_ms {
            0..=1500 => {
                button_sender
                    .send(crate::GpioInputEvents::NextTrigger)
                    .await
            }
            _ => button_sender.send(crate::GpioInputEvents::Retrigger).await,
        };
        defmt::debug!("Button interruption finished. Led off.");
        control.gpio_set(0, true).await;
    }
}
