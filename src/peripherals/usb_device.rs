//! USB HID Device Driver
//!
//! Safe abstraction over embassy-usb for creating HID devices on RP2040.
//! Supports keyboards, mice, or any custom HID device.
//!
//! # Example
//!
//! ```ignore
//! let config = UsbHidConfig {
//!     product: Some("USB Keyboard"),
//!     ..Default::default()
//! };
//!
//! let mut keyboard = UsbHidDevice::new_keyboard(p.USB, Irqs, &spawner, config)
//!     .await
//!     .expect("Failed to initialize USB keyboard");
//!
//! // Send HID reports
//! keyboard.send_report(&report).await?;
//! ```

use core::sync::atomic::{AtomicBool, Ordering};
use defmt::info;
use embassy_executor::Spawner;
use embassy_executor::task;
use embassy_rp::interrupt::typelevel::Binding;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_usb::class::hid::{HidBootProtocol, HidReaderWriter, HidSubclass, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::{Builder, Config, Handler};
use static_cell::StaticCell;
use usbd_hid::descriptor::{AsInputReport, SerializedDescriptor};

// ============================================================================
// ERROR HANDLING
// ============================================================================

/// USB HID device errors
#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum UsbHidError {
    #[error("Failed to spawn USB task")]
    TaskSpawnFailed,
    #[error("Failed to write HID report")]
    WriteFailed,
}

// ============================================================================
// USB DEVICE HANDLERS
// ============================================================================

/// Default HID request handler
///
/// Provides empty implementations for all request handler methods.
/// This is sufficient for most HID devices.
struct DefaultRequestHandler;

impl RequestHandler for DefaultRequestHandler {
    fn get_report(
        &mut self,
        _id: embassy_usb::class::hid::ReportId,
        _buf: &mut [u8],
    ) -> Option<usize> {
        None
    }

    fn set_report(&mut self, _id: embassy_usb::class::hid::ReportId, _data: &[u8]) -> OutResponse {
        OutResponse::Accepted
    }

    fn set_idle_ms(&mut self, _id: Option<embassy_usb::class::hid::ReportId>, _dur: u32) {}

    fn get_idle_ms(&mut self, _id: Option<embassy_usb::class::hid::ReportId>) -> Option<u32> {
        None
    }
}

/// Default USB device handler
///
/// Tracks USB device state and logs state transitions.
struct DefaultHandler {
    configured: AtomicBool,
}

impl DefaultHandler {
    fn new() -> Self {
        Self {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for DefaultHandler {
    fn enabled(&mut self, enabled: bool) {
        if enabled {
            info!("USB Device enabled");
        } else {
            info!("USB Device disabled");
        }
    }

    fn reset(&mut self) {
        info!("USB Bus reset");
    }

    fn addressed(&mut self, _addr: u8) {
        info!("USB Address set");
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            info!("USB Device configured");
        } else {
            info!("USB Device deconfigured");
        }
    }
}

// ============================================================================
// USB TASK
// ============================================================================

/// USB device task
///
/// Runs the USB device state machine. This must be spawned for USB to work.
#[task]
async fn usb_task(mut usb_device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) {
    usb_device.run().await
}

// ============================================================================
// USB HID DEVICE CONFIGURATION
// ============================================================================

/// Configuration for USB HID device
///
/// Uses builder pattern via struct literal update syntax.
///
/// # Example
///
/// ```ignore
/// let config = UsbHidConfig {
///     vendor_id: 0x1234,
///     product_id: 0x5678,
///     manufacturer: Some("My Company"),
///     product: Some("My Device"),
///     ..Default::default()
/// };
/// ```
pub struct UsbHidConfig {
    /// USB vendor ID
    pub vendor_id: u16,
    /// USB product ID
    pub product_id: u16,
    /// Manufacturer string
    pub manufacturer: Option<&'static str>,
    /// Product string
    pub product: Option<&'static str>,
    /// Serial number string
    pub serial_number: Option<&'static str>,
    /// Maximum power consumption (in 2mA units)
    pub max_power: u8,
    /// Maximum packet size for endpoint 0
    pub max_packet_size: u8,
}

impl Default for UsbHidConfig {
    fn default() -> Self {
        Self {
            vendor_id: 0xc0de,
            product_id: 0xcafe,
            manufacturer: None,
            product: None,
            serial_number: None,
            max_power: 100,
            max_packet_size: 64,
        }
    }
}

// ============================================================================
// USB HID DEVICE
// ============================================================================

/// USB HID Device (generic - supports keyboard, mouse, or custom HID)
///
/// Provides a low-level API for sending HID reports.
///
/// # Example
///
/// ```ignore
/// let mut keyboard = UsbHidDevice::new_keyboard(p.USB, Irqs, &spawner, config)
///     .await
///     .expect("Failed to initialize USB keyboard");
///
/// // Send HID reports (caller constructs report)
/// let report = KeyboardReport { ... };
/// keyboard.send_report(&report).await?;
/// ```
pub struct UsbHidDevice {
    writer: embassy_usb::class::hid::HidWriter<'static, Driver<'static, USB>, 8>,
}

impl UsbHidDevice {
    /// Create a new USB HID device with a custom report descriptor
    ///
    /// This is the generic constructor that accepts any HID report descriptor.
    /// Use this for custom HID devices.
    ///
    /// # Arguments
    ///
    /// * `usb` - USB peripheral
    /// * `irqs` - Interrupt handler (from bind_interrupts!)
    /// * `spawner` - Embassy task spawner
    /// * `config` - USB device configuration
    /// * `report_descriptor` - HID report descriptor bytes
    ///
    /// # Example
    ///
    /// ```ignore
    /// let device = UsbHidDevice::new(
    ///     p.USB, Irqs, &spawner,
    ///     UsbHidConfig::default(),
    ///     MyCustomReportDescriptor::desc()
    /// ).await?;
    /// ```
    pub async fn new<I>(
        usb: embassy_rp::Peri<'static, USB>,
        irqs: I,
        spawner: &Spawner,
        config: UsbHidConfig,
        report_descriptor: &'static [u8],
    ) -> Result<Self, UsbHidError>
    where
        I: Binding<
                <USB as embassy_rp::usb::Instance>::Interrupt,
                embassy_rp::usb::InterruptHandler<USB>,
            >,
    {
        info!("Initializing USB HID device...");

        // Initialize USB driver
        let driver = Driver::new(usb, irqs);

        // USB configuration
        let mut usb_config = Config::new(config.vendor_id, config.product_id);
        usb_config.manufacturer = config.manufacturer;
        usb_config.product = config.product;
        usb_config.serial_number = config.serial_number;
        usb_config.max_power = config.max_power as u16;
        usb_config.max_packet_size_0 = config.max_packet_size;

        // Initialize static buffers (using StaticCell to avoid unsafe static mut)
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 256]> = StaticCell::new();

        let config_desc = CONFIG_DESCRIPTOR.init([0; 256]);
        let bos_desc = BOS_DESCRIPTOR.init([0; 256]);
        let msos_desc = MSOS_DESCRIPTOR.init([0; 256]);
        let control_buf = CONTROL_BUF.init([0; 256]);

        // Create USB builder with static buffers
        let mut builder = Builder::new(
            driver,
            usb_config,
            config_desc,
            bos_desc,
            msos_desc,
            control_buf,
        );

        // Static storage for HID state and request handler
        static HID_STATE: StaticCell<embassy_usb::class::hid::State<'static>> = StaticCell::new();
        static REQUEST_HANDLER: StaticCell<DefaultRequestHandler> = StaticCell::new();

        let hid_state = HID_STATE.init(embassy_usb::class::hid::State::new());
        let request_handler = REQUEST_HANDLER.init(DefaultRequestHandler);

        // HID class configuration
        let hid_config = embassy_usb::class::hid::Config {
            report_descriptor,
            request_handler: Some(request_handler),
            poll_ms: 60,
            max_packet_size: 64,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        };

        // Create HID reader/writer with state
        let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, hid_state, hid_config);

        // Create USB handler
        let _handler = DefaultHandler::new();

        // Build USB device
        let usb_device = builder.build();

        // Spawn USB task
        spawner.spawn(usb_task(usb_device).expect("failed to spawn usb_task"));

        // Split HID into reader and writer
        let (_reader, writer) = hid.split();

        info!("USB HID device initialized");

        Ok(Self { writer })
    }

    /// Create a new USB HID keyboard device
    ///
    /// Convenience method that uses the standard keyboard report descriptor.
    ///
    /// # Arguments
    ///
    /// * `usb` - USB peripheral
    /// * `irqs` - Interrupt handler (from bind_interrupts!)
    /// * `spawner` - Embassy task spawner
    /// * `config` - USB device configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = UsbHidConfig {
    ///     product: Some("USB Keyboard"),
    ///     ..Default::default()
    /// };
    ///
    /// let mut keyboard = UsbHidDevice::new_keyboard(p.USB, Irqs, &spawner, config)
    ///     .await
    ///     .expect("Failed to initialize USB keyboard");
    /// ```
    pub async fn new_keyboard<I>(
        usb: embassy_rp::Peri<'static, USB>,
        irqs: I,
        spawner: &Spawner,
        config: UsbHidConfig,
    ) -> Result<Self, UsbHidError>
    where
        I: Binding<
                <USB as embassy_rp::usb::Instance>::Interrupt,
                embassy_rp::usb::InterruptHandler<USB>,
            >,
    {
        Self::new(
            usb,
            irqs,
            spawner,
            config,
            usbd_hid::descriptor::KeyboardReport::desc(),
        )
        .await
    }

    /// Create a new USB HID mouse device
    ///
    /// Convenience method that uses the standard mouse report descriptor.
    ///
    /// # Arguments
    ///
    /// * `usb` - USB peripheral
    /// * `irqs` - Interrupt handler (from bind_interrupts!)
    /// * `spawner` - Embassy task spawner
    /// * `config` - USB device configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = UsbHidConfig {
    ///     product: Some("USB Mouse"),
    ///     ..Default::default()
    /// };
    ///
    /// let mut mouse = UsbHidDevice::new_mouse(p.USB, Irqs, &spawner, config)
    ///     .await
    ///     .expect("Failed to initialize USB mouse");
    /// ```
    pub async fn new_mouse<I>(
        usb: embassy_rp::Peri<'static, USB>,
        irqs: I,
        spawner: &Spawner,
        config: UsbHidConfig,
    ) -> Result<Self, UsbHidError>
    where
        I: Binding<
                <USB as embassy_rp::usb::Instance>::Interrupt,
                embassy_rp::usb::InterruptHandler<USB>,
            >,
    {
        Self::new(
            usb,
            irqs,
            spawner,
            config,
            usbd_hid::descriptor::MouseReport::desc(),
        )
        .await
    }

    /// Send a HID report
    ///
    /// Low-level API that sends a HID report. The report type must implement
    /// the `AsInputReport` trait (e.g., KeyboardReport, MouseReport).
    ///
    /// # Arguments
    ///
    /// * `report` - HID report (must implement AsInputReport)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Keyboard report
    /// let report = KeyboardReport {
    ///     modifier: 0,
    ///     reserved: 0,
    ///     keycodes: [0x04, 0, 0, 0, 0, 0], // 'a' key
    ///     leds: 0,
    /// };
    /// keyboard.send_report(&report).await?;
    /// ```
    pub async fn send_report<R: AsInputReport>(&mut self, report: &R) -> Result<(), UsbHidError> {
        self.writer
            .write_serialize(report)
            .await
            .map_err(|_| UsbHidError::WriteFailed)
    }
}
