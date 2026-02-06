use wasm_bindgen::prelude::*;

/// Result of the sail-or-motor calculation.
#[wasm_bindgen]
pub struct SailResult {
    /// Which scenario: "sail_only", "sail_and_motor", "motor_only", "motor_late"
    scenario: String,
    /// Formatted result text (HTML)
    result_html: String,
}

#[wasm_bindgen]
impl SailResult {
    #[wasm_bindgen(getter)]
    pub fn scenario(&self) -> String {
        self.scenario.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn result_html(&self) -> String {
        self.result_html.clone()
    }
}

fn format_time(hours: f64) -> String {
    let whole_hours = hours.floor() as u32;
    let minutes = ((hours - hours.floor()) * 60.0).round() as u32;
    if minutes > 0 {
        format!("{} uur en {} minuten", whole_hours, minutes)
    } else {
        format!("{} uur", whole_hours)
    }
}

fn format_clock(total_minutes: i32) -> String {
    let h = ((total_minutes / 60) % 24 + 24) % 24;
    let m = ((total_minutes % 60) + 60) % 60;
    format!("{:02}:{:02}", h, m)
}

/// Core calculation matching the Vue SailingCalculator.vue logic exactly.
///
/// Parameters:
/// - start_time, arrival_time: "HH:MM" strings
/// - distance: in current unit (zeemijl or km)
/// - sail_speed: in current unit (knopen or km/u)
/// - motor_speed: in current unit (knopen or km/u)
/// - fuel_consumption: liters per hour
/// - use_metric: if true, convert km→zeemijl internally
/// - is_next_day: if true, add 24h to total time
/// - unit_label: "zeemijl" or "km"
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

    // Display values in original units
    let dist_display = distance;

    // Step 1: Can sailing alone cover the distance?
    let sail_time_limit = dist / s_speed;
    if sail_time_limit <= total_time {
        let html = format!(
            "Je kunt de hele afstand zeilen in {} ({:.2} {}).\
            <br>Geschat brandstofverbruik: 0 liter.",
            format_time(sail_time_limit),
            dist_display,
            unit
        );
        return SailResult {
            scenario: "sail_only".to_string(),
            result_html: html,
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

        let html = format!(
            "Je kunt {} zeilen ({:.2} {}).\
            Daarna moet je overschakelen naar de motor voor de resterende {:.2} {},\
            wat {} duurt.\
            <br><br>Geschat brandstofverbruik: {:.2} liter.",
            format_time(changeover_point),
            sailed_display,
            unit,
            remaining_display,
            unit,
            format_time(motoring_time),
            fuel
        );
        return SailResult {
            scenario: "sail_and_motor".to_string(),
            result_html: html,
        };
    }

    // Step 4: Motor only — check if on time
    let time_to_motor_full = dist / m_speed;
    if time_to_motor_full > total_time {
        // Can't arrive on time
        let extra_time = time_to_motor_full - total_time;
        let actual_arrival_mins = start_mins + ((total_time + extra_time) * 60.0) as i32;
        let fuel = time_to_motor_full * fuel_consumption;

        let html = format!(
            "Je kunt de hele afstand op de motor afleggen, maar je zult niet op tijd aankomen.\
            <br>Je verwachte aankomsttijd is {}.\
            <br><br>Geschat brandstofverbruik: {:.2} liter.",
            format_clock(actual_arrival_mins),
            fuel
        );
        return SailResult {
            scenario: "motor_late".to_string(),
            result_html: html,
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
        scenario: "motor_only".to_string(),
        result_html: html,
    }
}

/// Validate that motor speed > sail speed. Returns error message or empty string.
#[wasm_bindgen]
pub fn validate_motor_speed(sail_speed: f64, motor_speed: f64) -> String {
    if motor_speed <= sail_speed {
        "Motorsnelheid moet groter zijn dan zeilsnelheid".to_string()
    } else {
        String::new()
    }
}

/// Check if arrival is before start (needs next-day dialog).
#[wasm_bindgen]
pub fn needs_next_day(start_time: &str, arrival_time: &str) -> bool {
    let start = parse_time(start_time).unwrap_or(0);
    let arrival = parse_time(arrival_time).unwrap_or(0);
    arrival <= start
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
        scenario: "error".to_string(),
        result_html: msg.to_string(),
    }
}
