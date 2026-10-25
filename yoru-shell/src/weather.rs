//! Weather, strictly opt-in (Ferrix's docs/DESKTOP.md §6.6, the
//! customer's terms of 2026-10-24): off until the person turns it on and
//! types a location; no request of any kind before that, none at start,
//! no location guessed from the address; the location kept in their own
//! file only; turning it off stops every request. What is sent, and to
//! whom: the coordinates, rounded to one decimal (about 10 km), to
//! api.open-meteo.com, or the place's name to its geocoding host,
//! geocoding-api.open-meteo.com, both Open-Meteo's.
//!
//! The widget that shows it is step Y4; this is the rule it is built on,
//! and what a request may be.

/// Where the weather is asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// A place's name, as the person typed it, looked up by Open-Meteo's
    /// geocoding.
    Named(String),
    /// Coordinates, rounded before they are kept.
    At {
        /// Latitude, one decimal.
        latitude: f64,
        /// Longitude, one decimal.
        longitude: f64,
    },
}

impl Place {
    /// Coordinates rounded to one decimal, as they are kept and sent.
    #[must_use]
    pub fn at(latitude: f64, longitude: f64) -> Self {
        let round = |value: f64| (value * 10.0).round() / 10.0;
        Place::At {
            latitude: round(latitude),
            longitude: round(longitude),
        }
    }
}

/// The person's weather setting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weather {
    /// Whether they turned it on.
    pub on: bool,
    /// The location they typed; `None` until they do.
    pub place: Option<Place>,
}

/// The host every weather request goes to, and nowhere else.
pub const HOST: &str = "api.open-meteo.com";

/// What the setting says is sent, to whom.
pub const SAID: &str = "Sends the coordinates you type, rounded to about 10 km, to \
     api.open-meteo.com, or the place's name to geocoding-api.open-meteo.com; nothing else.";

impl Weather {
    /// The request to make now, or `None`: only when it is on *and* a
    /// location was typed. Never a lookup of where this computer is.
    #[must_use]
    pub fn request(&self) -> Option<String> {
        if !self.on {
            return None;
        }
        match self.place.as_ref()? {
            Place::At {
                latitude,
                longitude,
            } => Some(format!(
                "https://{HOST}/v1/forecast?latitude={latitude:.1}&longitude={longitude:.1}\
                 &current=temperature_2m,apparent_temperature,weather_code,is_day\
                 &hourly=temperature_2m,weather_code&forecast_hours=12\
                 &daily=weather_code,temperature_2m_max,temperature_2m_min&forecast_days=7\
                 &timezone=auto"
            )),
            Place::Named(name) => Some(format!(
                "https://geocoding-api.open-meteo.com/v1/search?count=1&name={}",
                escaped(name.trim())
            )),
        }
    }
}

/// A place's name as a query's value: what is not a letter, a digit or one
/// of `-._~` escaped, a space as `+`.
fn escaped(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        match byte {
            b' ' => out.push('+'),
            b'-' | b'.' | b'_' | b'~' => out.push(char::from(byte)),
            _ if byte.is_ascii_alphanumeric() => out.push(char::from(byte)),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl Weather {
    /// The setting as its file holds it: `on`, and `at LAT LON` or `name
    /// TEXT`, and `label TEXT` for the place's name once it is found.
    #[must_use]
    pub fn read(text: &str) -> (Self, String) {
        let mut weather = Weather::default();
        let mut label = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line == "on" {
                weather.on = true;
            } else if let Some(rest) = line.strip_prefix("at ") {
                let mut parts = rest
                    .split_whitespace()
                    .filter_map(|p| p.parse::<f64>().ok());
                if let (Some(latitude), Some(longitude)) = (parts.next(), parts.next()) {
                    weather.place = Some(Place::at(latitude, longitude));
                }
            } else if let Some(rest) = line.strip_prefix("name ") {
                weather.place = Some(Place::Named(rest.trim().to_owned()));
            } else if let Some(rest) = line.strip_prefix("label ") {
                rest.trim().clone_into(&mut label);
            }
        }
        (weather, label)
    }

    /// The file's text for the setting and the place's `label`.
    #[must_use]
    pub fn text(&self, label: &str) -> String {
        let mut out = String::new();
        if self.on {
            out.push_str("on\n");
        }
        match &self.place {
            Some(Place::At {
                latitude,
                longitude,
            }) => out.push_str(&format!("at {latitude:.1} {longitude:.1}\n")),
            Some(Place::Named(name)) => out.push_str(&format!("name {name}\n")),
            None => {}
        }
        if !label.is_empty() {
            out.push_str(&format!("label {label}\n"));
        }
        out
    }
}

/// What a person types for a place: `lat, lon` as coordinates (rounded),
/// anything else a name to look up.
#[must_use]
pub fn place(typed: &str) -> Option<Place> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let numbers: Vec<f64> = typed
        .split([',', ' '])
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<f64>().ok())
        .collect();
    match numbers.as_slice() {
        [latitude, longitude]
            if typed.split([',', ' ']).filter(|p| !p.is_empty()).count() == 2
                && latitude.abs() <= 90.0
                && longitude.abs() <= 180.0 =>
        {
            Some(Place::at(*latitude, *longitude))
        }
        _ => Some(Place::Named(typed.to_owned())),
    }
}

/// The first place a geocoding answer names: its coordinates, rounded, and
/// its name with its country.
#[must_use]
pub fn found(answer: &crate::json::Json) -> Option<(Place, String)> {
    let first = answer.get("results")?.items().first()?;
    let latitude = first.get("latitude")?.num()?;
    let longitude = first.get("longitude")?.num()?;
    let name = first.get("name")?.str()?;
    let label = match first.get("country").and_then(crate::json::Json::str) {
        Some(country) => format!("{name}, {country}"),
        None => name.to_owned(),
    };
    Some((Place::at(latitude, longitude), label))
}

/// A WMO weather code as the widget says it, and Phosphor's icon for it
/// (day or night).
#[must_use]
pub fn condition(code: i64, day: bool) -> (&'static str, &'static str) {
    match code {
        0 => ("Clear", if day { "sun" } else { "moon-stars" }),
        1 | 2 => (
            "Partly cloudy",
            if day { "cloud-sun" } else { "cloud-moon" },
        ),
        3 => ("Overcast", "cloud"),
        45 | 48 => ("Fog", "cloud-fog"),
        51..=57 => ("Drizzle", "cloud-rain"),
        61..=67 | 80..=82 => ("Rain", "cloud-rain"),
        71..=77 | 85 | 86 => ("Snow", "cloud-snow"),
        95..=99 => ("Thunderstorm", "cloud-lightning"),
        _ => ("Unknown", "cloud"),
    }
}

/// A forecast, as the widget shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forecast {
    /// The temperature now, °C.
    pub now: f64,
    /// What it feels like.
    pub feels: f64,
    /// The weather code now.
    pub code: i64,
    /// Day or night now.
    pub day: bool,
    /// The coming days: weekday (`Mon`), code, high and low.
    pub days: Vec<(String, i64, f64, f64)>,
    /// The coming hours: the hour (`14`), code, temperature.
    pub hours: Vec<(String, i64, f64)>,
}

/// The forecast in an Open-Meteo answer.
#[must_use]
pub fn forecast(answer: &crate::json::Json) -> Option<Forecast> {
    use crate::json::Json;
    let current = answer.get("current")?;
    let numbers = |of: &Json, key: &str| -> Vec<f64> {
        of.get(key)
            .map(|list| list.items().iter().filter_map(Json::num).collect())
            .unwrap_or_default()
    };
    let words = |of: &Json, key: &str| -> Vec<String> {
        of.get(key)
            .map(|list| {
                list.items()
                    .iter()
                    .filter_map(|v| v.str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut made = Forecast {
        now: current.get("temperature_2m")?.num()?,
        feels: current
            .get("apparent_temperature")
            .and_then(Json::num)
            .unwrap_or(f64::NAN),
        code: current
            .get("weather_code")
            .and_then(Json::int)
            .unwrap_or(-1),
        day: current.get("is_day").and_then(Json::int).unwrap_or(1) == 1,
        ..Forecast::default()
    };
    if let Some(daily) = answer.get("daily") {
        let dates = words(daily, "time");
        let codes = numbers(daily, "weather_code");
        let highs = numbers(daily, "temperature_2m_max");
        let lows = numbers(daily, "temperature_2m_min");
        for (i, date) in dates.iter().enumerate() {
            let (Some(code), Some(high), Some(low)) = (codes.get(i), highs.get(i), lows.get(i))
            else {
                continue;
            };
            #[expect(clippy::cast_possible_truncation, reason = "a WMO code")]
            made.days.push((weekday(date), *code as i64, *high, *low));
        }
    }
    if let Some(hourly) = answer.get("hourly") {
        let times = words(hourly, "time");
        let codes = numbers(hourly, "weather_code");
        let temps = numbers(hourly, "temperature_2m");
        for (i, time) in times.iter().enumerate() {
            let (Some(code), Some(temp)) = (codes.get(i), temps.get(i)) else {
                continue;
            };
            let hour = time
                .split('T')
                .nth(1)
                .and_then(|t| t.get(..2))
                .unwrap_or("");
            #[expect(clippy::cast_possible_truncation, reason = "a WMO code")]
            made.hours.push((hour.to_owned(), *code as i64, *temp));
        }
    }
    Some(made)
}

/// `2026-10-25`'s weekday, `Sun`.
fn weekday(date: &str) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    let mut parts = date.split('-').filter_map(|p| p.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return String::new();
    };
    let days = crate::widgets::days_from_civil(y, m, d);
    DAYS.get(usize::try_from(days.rem_euclid(7)).unwrap_or(0))
        .copied()
        .unwrap_or("")
        .to_owned()
}

/// Ask Open-Meteo: the place's coordinates first where a name was typed,
/// then the forecast. Through curl, which the image has; `YORU_WEATHER_FAKE`
/// names a folder whose `geocoding.json` and `forecast.json` stand in for
/// the two hosts, for a boot with no network. `None` from a weather that is
/// off: nothing is asked.
///
/// # Errors
///
/// What failed, as the widget says it.
pub fn fetch(weather: &Weather) -> Result<(Option<(Place, String)>, Forecast), String> {
    use crate::json::Json;
    let mut weather = weather.clone();
    let mut found = None;
    if matches!(weather.place, Some(Place::Named(_))) {
        let url = weather.request().ok_or("Weather is off")?;
        let answer = get(&url)?;
        let (place, label) = found_in(&answer).ok_or("Open-Meteo knows no such place")?;
        weather.place = Some(place.clone());
        found = Some((place, label));
    }
    let url = weather.request().ok_or("Weather is off")?;
    let answer = get(&url)?;
    let json = Json::parse(&answer).ok_or("Open-Meteo's answer was not read")?;
    let made = forecast(&json).ok_or("Open-Meteo's answer had no forecast")?;
    Ok((found, made))
}

/// The geocoding answer's first place.
fn found_in(answer: &str) -> Option<(Place, String)> {
    found(&crate::json::Json::parse(answer)?)
}

/// The body at `url`, from Open-Meteo's hosts alone.
fn get(url: &str) -> Result<String, String> {
    let host = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    if host != HOST && host != GEOCODING {
        return Err(format!("{host} is not Open-Meteo"));
    }
    crate::window::say(&format!("weather: asking {host}"));
    if let Ok(folder) = std::env::var("YORU_WEATHER_FAKE") {
        let file = if host == GEOCODING {
            "geocoding.json"
        } else {
            "forecast.json"
        };
        return std::fs::read_to_string(std::path::Path::new(&folder).join(file))
            .map_err(|error| format!("{file}: {error}"));
    }
    let output = std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "15", url])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|_| "No curl to ask Open-Meteo with".to_owned())?;
    if !output.status.success() {
        return Err("No answer from Open-Meteo".to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The geocoding host.
pub const GEOCODING: &str = "geocoding-api.open-meteo.com";

/// A temperature as the widget says it: `14°`.
#[must_use]
pub fn degrees(value: f64) -> String {
    if value.is_nan() {
        "–".to_owned()
    } else {
        format!("{}°", value.round())
    }
}

#[cfg(test)]
mod tests {
    use super::{Place, Weather};

    /// A shell nobody configured asks for no weather at all.
    #[test]
    fn an_unconfigured_shell_makes_no_weather_request() {
        assert_eq!(Weather::default().request(), None);
        let typed_but_off = Weather {
            on: false,
            place: Some(Place::at(52.52, 13.405)),
        };
        assert_eq!(typed_but_off.request(), None, "off stops every request");
        let on_without_a_place = Weather {
            on: true,
            place: None,
        };
        assert_eq!(on_without_a_place.request(), None, "no location is guessed");
    }

    #[test]
    fn the_setting_reads_back_and_a_typed_place_is_read() {
        let weather = Weather {
            on: true,
            place: Some(Place::at(35.0116, 135.7681)),
        };
        let (back, label) = Weather::read(&weather.text("Kyoto, Japan"));
        assert_eq!(back, weather);
        assert_eq!(label, "Kyoto, Japan");
        assert_eq!(
            super::place(" 35.01, 135.77 "),
            Some(Place::at(35.0, 135.8))
        );
        assert_eq!(super::place("Kyoto"), Some(Place::Named("Kyoto".into())));
        assert_eq!(super::place("200 5"), Some(Place::Named("200 5".into())));
        assert_eq!(super::place(""), None);
        let named = Weather {
            on: true,
            place: Some(Place::Named("São Paulo".into())),
        };
        assert!(
            named
                .request()
                .expect("asked")
                .ends_with("name=S%C3%A3o+Paulo")
        );
    }

    #[test]
    fn open_meteo_s_answers_are_read() {
        let geo = crate::json::Json::parse(
            r#"{"results":[{"name":"Kyoto","latitude":35.02107,"longitude":135.75385,"country":"Japan"}]}"#,
        )
        .expect("json");
        assert_eq!(
            super::found(&geo),
            Some((Place::at(35.0, 135.8), "Kyoto, Japan".to_owned()))
        );
        let answer = crate::json::Json::parse(
            r#"{"current":{"temperature_2m":14.3,"apparent_temperature":12.1,"weather_code":0,"is_day":0},
                "hourly":{"time":["2026-10-25T21:00","2026-10-25T22:00"],"temperature_2m":[14.0,13.5],"weather_code":[0,1]},
                "daily":{"time":["2026-10-25","2026-10-26"],"weather_code":[0,61],"temperature_2m_max":[17.2,15.0],"temperature_2m_min":[9.4,8.0]}}"#,
        )
        .expect("json");
        let made = super::forecast(&answer).expect("read");
        assert_eq!(super::degrees(made.now), "14°");
        assert_eq!(
            super::condition(made.code, made.day),
            ("Clear", "moon-stars")
        );
        assert_eq!(made.days.len(), 2);
        assert_eq!(made.days[0].0, "Sun");
        assert_eq!(super::condition(made.days[1].1, true).0, "Rain");
        assert_eq!(made.hours[1], ("22".to_owned(), 1, 13.5));
    }

    /// Coordinates go out rounded to one decimal.
    #[test]
    fn coordinates_are_coarse() {
        let weather = Weather {
            on: true,
            place: Some(Place::at(52.5247, 13.4049)),
        };
        let request = weather.request().expect("asked");
        assert!(request.starts_with("https://api.open-meteo.com/"));
        assert!(
            request.contains("latitude=52.5&longitude=13.4"),
            "{request}"
        );
    }
}
