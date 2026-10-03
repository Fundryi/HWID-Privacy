//! Local timestamps preserve the C# date and export filename formats.

use super::{Error, Result};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, SystemTimeToTzSpecificLocalTimeEx,
};

/// Formats Unix seconds in local time as yyyy-MM-dd HH:mm:ss.
pub fn unix_to_local(secs: i64) -> Option<String> {
    // C# parity: Hardware/SystemInfo.cs:77
    match local_time(secs, None) {
        Ok(local) => Some(display_time(&local)),
        Err(error) => {
            eprintln!("{error}");
            None
        }
    }
}
/// Returns local dd.MM.yyyy and HH;mm;ss filename components.
pub fn export_stamp() -> (String, String) {
    // C# parity: Services/FileExportService.cs:20
    // Keep the separate DateTime.Now reads used for the two filename components.
    // SAFETY: GetLocalTime has no caller-owned pointers and cannot fail.
    let date = unsafe { GetLocalTime() };
    // SAFETY: GetLocalTime has no caller-owned pointers and cannot fail.
    let time = unsafe { GetLocalTime() };
    export_parts(&date, &time)
}

fn local_time(secs: i64, zone: Option<&DYNAMIC_TIME_ZONE_INFORMATION>) -> Result<SYSTEMTIME> {
    let utc = utc_time(secs)?;
    let mut local = SYSTEMTIME::default();
    // SAFETY: The optional zone, UTC input and local output remain valid for this call.
    // A null zone uses the current Windows zone and its historical daylight-saving rules.
    unsafe { SystemTimeToTzSpecificLocalTimeEx(zone.map(std::ptr::from_ref), &utc, &mut local) }
        .map_err(|e| Error::from_win("SystemTimeToTzSpecificLocalTimeEx", e))?;
    if !(1..=9999).contains(&local.wYear) {
        return Err(Error::msg(
            "unix_to_local",
            "local timestamp is outside years 0001 through 9999",
        ));
    }
    Ok(local)
}

fn utc_time(secs: i64) -> Result<SYSTEMTIME> {
    // DateTimeOffset.FromUnixTimeSeconds accepts exactly this Gregorian range.
    if !(-62_135_596_800..=253_402_300_799).contains(&secs) {
        return Err(Error::msg(
            "unix_to_local",
            "Unix timestamp is outside years 0001 through 9999",
        ));
    }
    let unix_days = secs.div_euclid(86400);
    let seconds = secs.rem_euclid(86400);
    let mut days = unix_days + 719162;
    // Decompose the proleptic Gregorian calendar into 400/100/4/1-year cycles.
    let mut year = (days / 146097) * 400 + 1;
    days %= 146097;
    let centuries = (days / 36524).min(3);
    year += centuries * 100;
    days -= centuries * 36524;
    year += (days / 1461) * 4;
    days %= 1461;
    let years = (days / 365).min(3);
    year += years;
    days -= years * 365;
    let leap_day = i64::from(year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
    let mut month = 1;
    for length in [31, 28 + leap_day, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31] {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    Ok(SYSTEMTIME {
        wYear: year as u16,
        wMonth: month,
        wDay: (days + 1) as u16,
        wDayOfWeek: (unix_days + 4).rem_euclid(7) as u16,
        wHour: (seconds / 3600) as u16,
        wMinute: ((seconds % 3600) / 60) as u16,
        wSecond: (seconds % 60) as u16,
        wMilliseconds: 0,
    })
}

fn display_time(local: &SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute, local.wSecond
    )
}

fn export_parts(date: &SYSTEMTIME, time: &SYSTEMTIME) -> (String, String) {
    (
        format!("{:02}.{:02}.{:04}", date.wDay, date.wMonth, date.wYear),
        format!("{:02};{:02};{:02}", time.wHour, time.wMinute, time.wSecond),
    )
}

#[cfg(test)]
mod tests {
    use super::super::wide;
    use super::*;
    use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Time::EnumDynamicTimeZoneInformation;

    fn named_zone(name: &str) -> DYNAMIC_TIME_ZONE_INFORMATION {
        for index in 0..u32::MAX {
            let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
            // SAFETY: zone is writable; enumeration reads installed timezone definitions only.
            let status = unsafe { EnumDynamicTimeZoneInformation(index, &mut zone) };
            assert_ne!(
                status, ERROR_NO_MORE_ITEMS.0,
                "missing Windows zone: {name}"
            );
            assert_eq!(status, ERROR_SUCCESS.0, "timezone enumeration failed");
            if wide::from_wide(&zone.TimeZoneKeyName) == name {
                return zone;
            }
        }
        panic!("timezone index exhausted");
    }

    #[test]
    fn fixed_instants_cover_leap_day_negative_seconds_and_export_formats() {
        let zone = named_zone("India Standard Time");
        let local = local_time(1_709_164_800, Some(&zone)).expect("Windows check should succeed");
        assert_eq!(display_time(&local), "2024-02-29 05:30:00");
        assert_eq!(
            export_parts(&local, &local),
            ("29.02.2024".to_owned(), "05;30;00".to_owned())
        );
        let utc = named_zone("UTC");
        assert_eq!(
            display_time(&local_time(-1, Some(&utc)).expect("Windows check should succeed")),
            "1969-12-31 23:59:59"
        );
        assert_eq!(
            display_time(&utc_time(-62_135_596_800).expect("Windows check should succeed")),
            "0001-01-01 00:00:00"
        );
        assert_eq!(
            display_time(&utc_time(253_402_300_799).expect("Windows check should succeed")),
            "9999-12-31 23:59:59"
        );
        assert_eq!(
            display_time(&utc_time(-2_208_988_800).expect("Windows check should succeed")),
            "1900-01-01 00:00:00"
        );
        assert_eq!(
            display_time(&utc_time(951_782_400).expect("Windows check should succeed")),
            "2000-02-29 00:00:00"
        );
        assert!(unix_to_local(i64::MIN).is_none());
        assert!(unix_to_local(i64::MAX).is_none());
        let local = unix_to_local(1_709_164_800).expect("current Windows zone must resolve");
        println!("Current-zone fixed instant: {local}");
        let (date, time) = export_stamp();
        assert_eq!(date.len(), 10);
        assert_eq!(time.len(), 8);
    }

    #[test]
    fn fixed_instant_observes_the_daylight_saving_transition() {
        let zone = named_zone("Eastern Standard Time");
        assert_eq!(
            display_time(
                &local_time(1_710_053_999, Some(&zone)).expect("Windows check should succeed")
            ),
            "2024-03-10 01:59:59"
        );
        assert_eq!(
            display_time(
                &local_time(1_710_054_000, Some(&zone)).expect("Windows check should succeed")
            ),
            "2024-03-10 03:00:00"
        );
    }
}
