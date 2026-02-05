use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::warn;
use embassy_net::{Stack, StackResources};
use embassy_rp::gpio::Output;
use embassy_rp::interrupt::typelevel::Binding;
use embassy_rp::pio::{InterruptHandler, Pio};
use static_cell::StaticCell;

const CYW43_FW: &[u8] = include_bytes!("./cyw43-firmware/43439A0.bin");
const CYW43_CLM: &[u8] = include_bytes!("./cyw43-firmware/43439A0_clm.bin");

pub struct WifiPins {
    pub pwr: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_23>,
    pub cs: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_25>,
    pub clk: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_29>,
    pub dio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_24>,
    pub pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
    pub dma: embassy_rp::Peri<'static, embassy_rp::peripherals::DMA_CH0>,
}

pub struct PioKeepalive<'a> {
    _common: embassy_rp::pio::Common<'a, embassy_rp::peripherals::PIO0>,
    _irq_flags: embassy_rp::pio::IrqFlags<'a, embassy_rp::peripherals::PIO0>,
    _irq1: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 1>,
    _irq2: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 2>,
    _irq3: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 3>,
    _sm1: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 1>,
    _sm2: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 2>,
    _sm3: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 3>,
}

pub struct WifiConfig {
    pub power_mode: cyw43::PowerManagementMode,
    pub stack_config: embassy_net::Config,
}

pub struct WifiManager {
    pub control: cyw43::Control<'static>,
    pub stack: Stack<'static>,
    _pio_keepalive: PioKeepalive<'static>,
}

impl WifiManager {
    pub async fn init_wifi(
        pins: WifiPins,
        irqs: impl Binding<
            embassy_rp::interrupt::typelevel::PIO0_IRQ_0,
            InterruptHandler<embassy_rp::peripherals::PIO0>,
        >,
        config: WifiConfig,
        spawner: embassy_executor::Spawner,
    ) -> WifiManager {
        // Create WiFi control pins from peripherals
        let pwr = Output::new(pins.pwr, embassy_rp::gpio::Level::Low);
        let cs = Output::new(pins.cs, embassy_rp::gpio::Level::High);

        // 1. Initialize CYW43 WiFi chip
        let mut pio = Pio::new(pins.pio, irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            pins.dio,
            pins.clk,
            pins.dma,
        );
        let pio_keepalive = PioKeepalive {
            _common: pio.common,
            _irq_flags: pio.irq_flags,
            _irq1: pio.irq1,
            _irq2: pio.irq2,
            _irq3: pio.irq3,
            _sm1: pio.sm1,
            _sm2: pio.sm2,
            _sm3: pio.sm3,
        };

        static STATE: StaticCell<cyw43::State> = StaticCell::new();
        let state = STATE.init(cyw43::State::new());
        let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, CYW43_FW).await;

        spawner.spawn(cyw43_runner_task(runner).expect("failed to spawn cyw43_runner_task"));

        control.init(CYW43_CLM).await;
        control.set_power_management(config.power_mode).await;

        // 2. Initialize network stack
        let mut rng = embassy_rp::clocks::RoscRng;
        let seed = rng.next_u64();

        static RESOURCES: StaticCell<StackResources<6>> = StaticCell::new();
        let (stack, runner) = embassy_net::new(
            net_device,
            config.stack_config,
            RESOURCES.init(StackResources::new()),
            seed,
        );

        // Spawn the network runner task
        spawner.spawn(net_runner_task(runner).expect("failed to spawn net_runner_task"));

        WifiManager {
            control,
            stack,
            _pio_keepalive: pio_keepalive,
        }
    }

    pub async fn join_network(&mut self, wifi_ssid: &str, wifi_password: &str) {
        loop {
            match self
                .control
                .join(wifi_ssid, cyw43::JoinOptions::new(wifi_password.as_bytes()))
                .await
            {
                Ok(()) => break,
                Err(_) => {
                    warn!("WiFi join failed, retrying...");
                }
            }
        }
        self.stack.wait_link_up().await;
        self.stack.wait_config_up().await;
    }

    pub async fn start_ap_wpa2(&mut self, ap_ssid: &str, ap_password: &str, channel: u8) {
        self.control
            .start_ap_wpa2(ap_ssid, ap_password, channel)
            .await;
    }
}

#[embassy_executor::task]
async fn cyw43_runner_task(
    runner: cyw43::Runner<
        'static,
        Output<'static>,
        PioSpi<'static, embassy_rp::peripherals::PIO0, 0, embassy_rp::peripherals::DMA_CH0>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_runner_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}
