use wasm_bindgen::prelude::*;

/// Result of the sail-or-motor calculation.
#[wasm_bindgen]
pub struct SailResult {
    // Using numeric scenario codes to avoid string allocations (#9)
    // 0=error, 1=sail_only, 2=sail_and_motor, 3=motor_only, 4=motor_late
    scenario: u8,
    result_html: String,
    // Structured data for the visual timeline (#19)
    sail_fraction: f64,
    motor_fraction: f64,
}

#[wasm_bindgen]
impl SailResult {
    #[wasm_bindgen(getter)]
    pub fn scenario(&self) -> u8 {
        self.scenario
    }
    #[wasm_bindgen(getter)]
    pub fn result_html(&self) -> String {
        self.result_html.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn sail_fraction(&self) -> f64 {
        self.sail_fraction
    }
    #[wasm_bindgen(getter)]
    pub fn motor_fraction(&self) -> f64 {
        self.motor_fraction
    }
}

const SCENARIO_ERROR: u8 = 0;
const SCENARIO_SAIL_ONLY: u8 = 1;
const SCENARIO_SAIL_AND_MOTOR: u8 = 2;
const SCENARIO_MOTOR_ONLY: u8 = 3;
const SCENARIO_MOTOR_LATE: u8 = 4;

/// Format hours as "X uur en Y minuten", handling the 60-minute rollover (#1)
fn format_time(hours: f64) -> String {
    let total_minutes = (hours * 60.0).round() as u32;
    let h = total_minutes / 60;
    let m = total_minutes % 60;
    if m > 0 {
        format!("{} uur en {} minuten", h, m)
    } else {
        format!("{} uur", h)
    }
}

fn format_clock(total_minutes: i32) -> String {
    let h = ((total_minutes / 60) % 24 + 24) % 24;
    let m = ((total_minutes % 60) + 60) % 60;
    format!("{:02}:{:02}", h, m)
}

/// Core calculation matching the Vue SailingCalculator.vue logic.
///
/// Parameters:
/// - start_time, arrival_time: "HH:MM" strings
/// - distance: in current unit (zeemijl or km)
/// - sail_speed: in current unit (knopen or km/u)
/// - motor_speed: in current unit (knopen or km/u)
/// - fuel_consumption: liters per hour
/// - use_metric: if true, convert km to zeemijl internally
/// - is_next_day: if true, add 24h to total time
#[wasm_bindgen]
pub fn calculate(
    start_time: &str,
    arrival_time: &str,
    distance: f64,
    sail_speed: f64,
    motor_speed: f64,
    fuel_consumption: f64,
    use_metric: bool,
    is_next_day: bool,
) -> SailResult {
    // Parse times
    let start_mins = match parse_time(start_time) {
        Some(m) => m as i32,
        None => return error_result("Ongeldige starttijd."),
    };
    let arrival_mins = match parse_time(arrival_time) {
        Some(m) => m as i32,
        None => return error_result("Ongeldige aankomsttijd."),
    };

    // Validate inputs (#5): prevent zero/negative causing division by zero
    if distance <= 0.0 {
        return error_result("Afstand moet groter zijn dan 0.");
    }
    if sail_speed <= 0.0 {
        return error_result("Zeilsnelheid moet groter zijn dan 0.");
    }
    if motor_speed <= 0.0 {
        return error_result("Motorsnelheid moet groter zijn dan 0.");
    }
    if motor_speed <= sail_speed {
        return error_result("Motorsnelheid moet groter zijn dan zeilsnelheid.");
    }

    // Total time in hours
    let mut diff_mins = arrival_mins - start_mins;
    if is_next_day {
        diff_mins += 24 * 60;
    }
    let total_time = diff_mins as f64 / 60.0;

    if total_time <= 0.0 {
        return error_result("Aankomsttijd moet later zijn dan starttijd.");
    }

    // Convert to nautical if metric
    let (dist, s_speed, m_speed) = if use_metric {
        (distance / 1.852, sail_speed / 1.852, motor_speed / 1.852)
    } else {
        (distance, sail_speed, motor_speed)
    };

    let unit = if use_metric { "km" } else { "zeemijl" };

    // Step 1: Can sailing alone cover the distance?
    let sail_time_limit = dist / s_speed;
    if sail_time_limit <= total_time {
        // Convert back for display
        let dist_display = if use_metric { dist * 1.852 } else { dist };
        let html = format!(
            "Je kunt de hele afstand zeilen in {} ({:.2} {}).\
             <br>Geschat brandstofverbruik: 0 liter.",
            format_time(sail_time_limit),
            dist_display,
            unit
        );
        return SailResult {
            scenario: SCENARIO_SAIL_ONLY,
            result_html: html,
            sail_fraction: 1.0,
            motor_fraction: 0.0,
        };
    }

    // Step 2: Calculate changeover point
    let motor_distance = m_speed * total_time;
    let difference = dist - motor_distance;
    let speed_difference = s_speed - m_speed;
    let changeover_point = difference / speed_difference;

    // Step 3: Sail + motor combination feasible?
    if changeover_point >= 0.0 && changeover_point <= total_time {
        let distance_sailed = s_speed * changeover_point;
        let remaining_distance = dist - distance_sailed;
        let motoring_time = remaining_distance / m_speed;
        let fuel = motoring_time * fuel_consumption;

        // Convert distances back for display
        let (sailed_display, remaining_display) = if use_metric {
            (distance_sailed * 1.852, remaining_distance * 1.852)
        } else {
            (distance_sailed, remaining_distance)
        };

        // Changeover clock time (#11)
        let changeover_clock_mins = start_mins + (changeover_point * 60.0).round() as i32;
        let changeover_clock = format_clock(changeover_clock_mins);

        // (#10): proper spacing between sentences
        let html = format!(
            "Je kunt {} zeilen ({:.2} {}). \
             Daarna moet je overschakelen naar de motor voor de resterende {:.2} {}, \
             wat {} duurt.\
             <br>Start de motor om <strong>{}</strong>.\
             <br><br>Geschat brandstofverbruik: {:.2} liter.",
            format_time(changeover_point),
            sailed_display,
            unit,
            remaining_display,
            unit,
            format_time(motoring_time),
            changeover_clock,
            fuel
        );
        return SailResult {
            scenario: SCENARIO_SAIL_AND_MOTOR,
            result_html: html,
            sail_fraction: changeover_point / total_time,
            motor_fraction: motoring_time / total_time,
        };
    }

    // Step 4: Motor only — check if on time
    let time_to_motor_full = dist / m_speed;
    if time_to_motor_full > total_time {
        // Can't arrive on time
        let actual_arrival_mins = start_mins + (time_to_motor_full * 60.0).round() as i32;
        let fuel = time_to_motor_full * fuel_consumption;

        let html = format!(
            "Je kunt de hele afstand op de motor afleggen, maar je zult niet op tijd aankomen. \
             <br>Je verwachte aankomsttijd is <strong>{}</strong>.\
             <br><br>Geschat brandstofverbruik: {:.2} liter.",
            format_clock(actual_arrival_mins),
            fuel
        );
        return SailResult {
            scenario: SCENARIO_MOTOR_LATE,
            result_html: html,
            sail_fraction: 0.0,
            motor_fraction: 1.0,
        };
    }

    // Step 5: Motor only, within time
    let fuel = time_to_motor_full * fuel_consumption;
    let html = format!(
        "Je kunt de hele afstand op de motor afleggen in {}.\
         <br><br>Geschat brandstofverbruik: {:.2} liter.",
        format_time(time_to_motor_full),
        fuel
    );
    SailResult {
        scenario: SCENARIO_MOTOR_ONLY,
        result_html: html,
        sail_fraction: 0.0,
        motor_fraction: time_to_motor_full / total_time,
    }
}

/// Validate motor speed > sail speed. Returns error message or empty string.
#[wasm_bindgen]
pub fn validate_motor_speed(sail_speed: f64, motor_speed: f64) -> String {
    if sail_speed <= 0.0 || motor_speed <= 0.0 {
        return String::new(); // Don't show cross-field error while still typing
    }
    if motor_speed <= sail_speed {
        "Motorsnelheid moet groter zijn dan zeilsnelheid".to_string()
    } else {
        String::new()
    }
}

/// Check if arrival is before start (needs next-day dialog).
/// Only triggers when both times are fully entered and arrival is strictly before start (#4).
#[wasm_bindgen]
pub fn needs_next_day(start_time: &str, arrival_time: &str) -> bool {
    let start = match parse_time(start_time) {
        Some(m) => m,
        None => return false,
    };
    let arrival = match parse_time(arrival_time) {
        Some(m) => m,
        None => return false,
    };
    arrival < start
}

/// Validate a numeric field against a max value. Returns error message or empty string (#6).
#[wasm_bindgen]
pub fn validate_max(value: f64, max: f64, field_name: &str, unit: &str) -> String {
    if value > max {
        format!("{} moet kleiner zijn dan {} {}", field_name, max, unit)
    } else {
        String::new()
    }
}

fn parse_time(time_str: &str) -> Option<u32> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hours: u32 = parts[0].parse().ok()?;
    let minutes: u32 = parts[1].parse().ok()?;
    if hours >= 24 || minutes >= 60 {
        return None;
    }
    Some(hours * 60 + minutes)
}

fn error_result(msg: &str) -> SailResult {
    SailResult {
        scenario: SCENARIO_ERROR,
        result_html: msg.to_string(),
        sail_fraction: 0.0,
        motor_fraction: 0.0,
    }
}
