//! Unit conversion utilities

// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/// Convert kilohertz to megahertz
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[inline]
pub fn khz_to_mhz(khz: u32) -> u32 {
    khz / 1000
}

/// Convert hertz to megahertz
#[inline]
pub fn hz_to_mhz(hz: u64) -> u32 {
    (hz / 1_000_000) as u32
}

/// Convert millicelsius to celsius
#[inline]
pub fn millicelsius_to_celsius(millicelsius: u32) -> u32 {
    millicelsius / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_khz_to_mhz() {
        assert_eq!(khz_to_mhz(1000), 1);
        assert_eq!(khz_to_mhz(2500), 2);
        assert_eq!(khz_to_mhz(3600), 3);
        assert_eq!(khz_to_mhz(0), 0);
    }

    #[test]
    fn test_hz_to_mhz() {
        assert_eq!(hz_to_mhz(1_000_000), 1);
        assert_eq!(hz_to_mhz(2_500_000), 2);
        assert_eq!(hz_to_mhz(3_600_000_000), 3600);
        assert_eq!(hz_to_mhz(0), 0);
    }

    #[test]
    fn test_millicelsius_to_celsius() {
        assert_eq!(millicelsius_to_celsius(1000), 1);
        assert_eq!(millicelsius_to_celsius(25000), 25);
        assert_eq!(millicelsius_to_celsius(100500), 100);
        assert_eq!(millicelsius_to_celsius(0), 0);
    }
}
