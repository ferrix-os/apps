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
                 &current=temperature_2m,weather_code&daily=temperature_2m_max,temperature_2m_min"
            )),
            Place::Named(name) => Some(format!(
                "https://geocoding-api.open-meteo.com/v1/search?count=1&name={}",
                name.trim().replace(' ', "+")
            )),
        }
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
