/*-
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * BSD 2-Clause License
 *
 * Copyright (c) 2021-2023, Gandi S.A.S.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice, this
 *    list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

#[macro_use]
extern crate lazy_static;

use std::net::SocketAddr;

use prometheus::{GaugeVec, IntGauge, IntGaugeVec, Opts, Registry};
use std::env;
use std::result::Result;
use warp::{Filter, Rejection, Reply};

mod jbod;
mod utils;
use crate::jbod::disks::DiskShelf;
use crate::jbod::enclosure::BackPlane;
use crate::utils::helper::Util;

// Declare code to be executed at runtime, this includes anything requiring
// heap allocations and function calls to be computed.
//
// Every exporter metrics are declared here first.
//
lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref NUMBER_OF_ENCLOSURES: IntGauge =
        IntGauge::new("number_of_enclosures", "Number of enclosures")
            .expect("metric can be created");
    pub static ref JBOD_SLOT_TEMPERATURE: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_slot_temperature",
            "Enclosure number, slot position and temperature"
        ),
        &["slot", "enclosure"]
    )
    .expect("metric can be created");
    pub static ref JBOD_ENCLOSURE_TEMPERATURE: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_enclosure_temperature",
            "Enclosure number, slot position, description, status and temperature"
        ),
        &["enclosure", "slot", "description", "status"]
    )
    .expect("metric can be created");
    pub static ref JBOD_ENCLOSURE_VOLTAGE: GaugeVec = GaugeVec::new(
        Opts::new(
            "jbod_enclosure_voltage",
            "Enclosure number, slot position, description, status and voltage"
        ),
        &["enclosure", "slot", "description", "status"]
    )
    .expect("metric can be created");
    pub static ref JBOD_FAN_RPM: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_fan_rpm",
            "The RPM speed of FAN components, device and slot"
        ),
        &["description", "slot", "comment"]
    )
    .expect("metric can be created");
    pub static ref JBOD_PSU_STATUS: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_psu_status",
            "Status of PSU components per enclosure slot and identifier"
        ),
        &["slot", "index", "description", "serial", "status"]
    )
    .expect("metric can be created");
    pub static ref JBOD_DISK_ATTRIBUTES: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_disk_attributes",
            "Disk inventory metadata keyed by enclosure and slot"
        ),
        &[
            "enclosure",
            "slot",
            "slot_label",
            "device_path",
            "device_map",
            "vendor",
            "model",
            "serial",
            "fw_revision"
        ]
    )
    .expect("metric can be created");
    pub static ref JBOD_DISK_LED_CAPABILITY: IntGaugeVec = IntGaugeVec::new(
        Opts::new(
            "jbod_disk_led_capability",
            "Presence of locate/fault LED control files per disk"
        ),
        &["enclosure", "slot", "device_path", "kind"]
    )
    .expect("metric can be created");
}

/// Here we register the metrics, this function is called in the `main()`.
fn register_metrics() {
    REGISTRY
        .register(Box::new(NUMBER_OF_ENCLOSURES.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_SLOT_TEMPERATURE.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_ENCLOSURE_TEMPERATURE.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_ENCLOSURE_VOLTAGE.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_FAN_RPM.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_PSU_STATUS.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_DISK_ATTRIBUTES.clone()))
        .expect("collector can be registered");
    REGISTRY
        .register(Box::new(JBOD_DISK_LED_CAPABILITY.clone()))
        .expect("collector can be registered");
}

// Index handler.
async fn index_handler() -> Result<impl Reply, Rejection> {
    Ok("")
}

/// Returns an `i64` with the total number of enclosures.
async fn number_of_enclosure_metrics() -> i64 {
    let enclosure = BackPlane::get_enclosure(false);
    return enclosure.len() as i64;
}

/// Returns Result with Reply and Rejection.
///
/// This function updates the prometheus-exporter metrics.
/// Also here we can find the logic behind each metric.
async fn metrics_handler() -> Result<impl Reply, Rejection> {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    // Enclosure PSU status (1 for OK, 0 otherwise)
    let mut enclosure_psus = BackPlane::get_enclosure_psus(false);
    enclosure_psus.sort_by_key(|p| (p.slot.clone(), p.index.clone()));
    for psu in enclosure_psus.iter() {
        let status_value = if psu.status.to_lowercase().contains("ok") {
            1
        } else {
            0
        };
        JBOD_PSU_STATUS
            .with_label_values(&[
                &psu.slot,
                &psu.index,
                &psu.description,
                &psu.serial,
                &psu.status,
            ])
            .set(status_value);
    }
    drop(enclosure_psus);

    // Enclosure FAN rpm
    let mut enclosure_fan = BackPlane::get_enclosure_fan(false);
    enclosure_fan.sort_by_key(|f| f.index.clone());
    for fan in enclosure_fan.iter() {
        JBOD_FAN_RPM
            .with_label_values(&[&fan.description, &fan.slot, &fan.comment])
            .set(fan.speed);
    }
    drop(enclosure_fan);

    // Enclosure Temperature sensors
    let mut enclosure_temp = BackPlane::get_enclosure_temp(false);
    enclosure_temp.sort_by_key(|f| f.index.clone());
    for temp in enclosure_temp.iter() {
        JBOD_ENCLOSURE_TEMPERATURE
            .with_label_values(&[&temp.slot, &temp.index, &temp.description, &temp.status])
            .set(temp.temperature);
    }
    drop(enclosure_temp);

    // Enclosure Voltage sensors
    let mut enclosure_voltage = BackPlane::get_enclosure_voltage(false);
    enclosure_voltage.sort_by_key(|f| f.index.clone());
    for voltage in enclosure_voltage.iter() {
        JBOD_ENCLOSURE_VOLTAGE
            .with_label_values(&[
                &voltage.slot,
                &voltage.index,
                &voltage.description,
                &voltage.status,
            ])
            .set(voltage.voltage);
    }
    drop(enclosure_voltage);

    // Enclosures
    let enclosures = number_of_enclosure_metrics();
    NUMBER_OF_ENCLOSURES.set(enclosures.await);

    // Disks slot temperature
    let mut disks_temperature = DiskShelf::jbod_disk_map();
    disks_temperature.sort_by_key(|d| d.slot.clone());
    for disk in disks_temperature.iter() {
        match disk.temperature.parse() {
            Ok(temperature) => JBOD_SLOT_TEMPERATURE
                .with_label_values(&[&disk.slot, &disk.enclosure])
                .set(temperature),
            Err(e) => eprintln!("Failed to read temperature: {:?} of disk: {:?}", e, disk),
        }

        JBOD_DISK_ATTRIBUTES
            .with_label_values(&[
                &disk.enclosure,
                &disk.slot,
                &disk.slot_label,
                &disk.device_path,
                &disk.device_map,
                &disk.vendor,
                &disk.model,
                &disk.serial,
                &disk.fw_revision,
            ])
            .set(1);

        let locate_status = if disk.led_locate_path != "NONE" { 1 } else { 0 };
        JBOD_DISK_LED_CAPABILITY
            .with_label_values(&[&disk.enclosure, &disk.slot, &disk.device_path, "locate"])
            .set(locate_status);

        let fault_status = if disk.led_fault_path != "NONE" { 1 } else { 0 };
        JBOD_DISK_LED_CAPABILITY
            .with_label_values(&[&disk.enclosure, &disk.slot, &disk.device_path, "fault"])
            .set(fault_status);
    }
    drop(disks_temperature);

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&REGISTRY.gather(), &mut buffer) {
        eprintln!("could not encode custom metrics: {}", e);
    };

    let mut res = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("custom metrics could not be from_utf8: {}", e);
            String::default()
        }
    };
    buffer.clear();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        eprintln!("could not encode prometheus metrics: {}", e);
    };
    let res_custom = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("prometheus metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    res.push_str(&res_custom);
    Ok(res)
}

/// `main()` function that starts the webserver.
#[tokio::main]
async fn main() {
    Util::verify_binary_needed();

    let args: Vec<String> = env::args().collect();
    let mut port: String = "9945".to_string();
    let mut ipv4: String = "0.0.0.0".to_string();

    if args.len() > 2 {
        if Util::is_string_numeric(&args[2]) {
            port = args[2].to_string();
        } else {
            println!("Port is not decimal, using default {}", port);
        }

        let t = args[1]
            .split(".")
            .map(Util::is_string_numeric)
            .any(|i| i == false);
        if !t {
            ipv4 = args[1].to_string();
        } else {
            println!("Using default ipv4: {}", ipv4);
        }
    }

    let adr: String = ipv4 + ":" + &port;
    let adr_convert: SocketAddr = adr.parse().expect("Could not parse SocketAddr");

    register_metrics();

    let metrics_route = warp::path!("metrics").and_then(metrics_handler);
    let route = warp::path::end().and_then(index_handler);

    println!("==> Started on {}", adr);
    warp::serve(metrics_route.or(route)).run(adr_convert).await;
}
