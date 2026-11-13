//! Distance-decay functions for accessibility, as R5 7.6
//! `com.conveyal.r5.analyst.decay.*`. Inputs are
//! whole seconds (cutoff and travel time); the weight is a double in
//! `[0, 1]`, computed with the same operations in the same order as the
//! Java bytecode, and with commons-math3's `FastMath.exp`, so the weights are
//! bit-identical to R5's.

use crate::fastmath;

/// `DecayFunction.TWO_HOURS_IN_SECONDS`.
const TWO_HOURS_IN_SECONDS: i32 = 7200;
/// `DecayFunction.FOUR_HOURS_IN_SECONDS`, the bisection's upper bound.
const FOUR_HOURS_IN_SECONDS: i32 = 14_400;
/// `DecayFunction.ZERO_EPSILON`: a weight below this counts as zero when
/// looking for the zero point.
const ZERO_EPSILON: f64 = 0.001;

/// A decay function and its parameter (the packages' `decay_function`
/// and `decay_value`): how much an opportunity reached in `t` counts
/// towards accessibility, given a cutoff. The weights are the same, to the
/// last bit, on every platform. Non-exhaustive: other functions may be
/// added.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DecayFunction {
    /// `step`, cumulative opportunities: weight 1 strictly below the
    /// cutoff, 0 from it on.
    Step,
    /// `exponential`: half-life = cutoff, `2^(-t / cutoff)`, 0 from two
    /// hours on.
    Exponential,
    /// `fixed_exponential`: `exp(-decay_constant * t)` with `t` in seconds,
    /// ignoring the cutoff.
    FixedExponential {
        /// `0 < decay_constant < 1`.
        decay_constant: f64,
    },
    /// `linear`: a linear ramp from 1 to 0 centred on the cutoff.
    Linear {
        /// Width of the ramp in minutes, `0..60`.
        width_minutes: i32,
    },
    /// `logistic`: a logistic CDF with median = cutoff.
    Logistic {
        /// Standard deviation in minutes, `> 0` and under two hours.
        standard_deviation_minutes: f64,
    },
}

/// A validated decay function, ready to evaluate (R5 after `prepare()`).
#[derive(Debug, Clone)]
pub struct Decay {
    function: DecayFunction,
    /// Linear: `widthMinutes * 60` and its half (integer division).
    width_seconds: i32,
    half_width_seconds: i32,
    /// Linear: weight of each second along the ramp.
    weight_table: Vec<f64>,
    /// Logistic: `standardDeviationMinutes * 60`.
    standard_deviation_seconds: f64,
}

/// `LogisticDecayFunction.SQRT3` = `FastMath.sqrt(3)` = `Math.sqrt(3)`.
fn sqrt3() -> f64 {
    3.0f64.sqrt()
}

/// `ExponentialDecayFunction.logOneHalf` = `FastMath.log(0.5)`, which the
/// jar evaluates to the correctly rounded ln(0.5) (`0xbfe62e42fefa39ef`).
const LOG_ONE_HALF: f64 = -std::f64::consts::LN_2;

impl Decay {
    /// Validate the parameter as each function's `prepare()` does.
    pub fn new(function: DecayFunction) -> Result<Decay, String> {
        let mut d = Decay {
            function: function.clone(),
            width_seconds: 0,
            half_width_seconds: 0,
            weight_table: Vec::new(),
            standard_deviation_seconds: 0.0,
        };
        match function {
            DecayFunction::Step | DecayFunction::Exponential => {}
            DecayFunction::FixedExponential { decay_constant } => {
                // FixedExponentialDecayFunction.prepare: 0 < k < 1 (NaN fails)
                if !(decay_constant > 0.0 && decay_constant < 1.0) {
                    return Err(format!(
                        "the fixed_exponential decay constant must be between 0 and 1 (exclusive), got {decay_constant}"
                    ));
                }
            }
            DecayFunction::Linear { width_minutes } => {
                // LinearDecayFunction.prepare
                if !(0..60).contains(&width_minutes) {
                    return Err(format!(
                        "the linear decay width must be 0 to 59 minutes, got {width_minutes}"
                    ));
                }
                d.width_seconds = width_minutes * 60;
                d.half_width_seconds = d.width_seconds / 2;
                d.weight_table = (0..d.width_seconds)
                    .map(|s| 1.0 - ((f64::from(s) + 0.5) / f64::from(d.width_seconds)))
                    .collect();
            }
            DecayFunction::Logistic {
                standard_deviation_minutes,
            } => {
                // LogisticDecayFunction.prepare: 0 < sd * 60 < 7200 (NaN fails)
                let sd = standard_deviation_minutes * 60.0;
                if !(sd > 0.0 && sd < f64::from(TWO_HOURS_IN_SECONDS)) {
                    return Err(format!(
                        "the logistic standard deviation must be positive and under 120 minutes, got {standard_deviation_minutes}"
                    ));
                }
                d.standard_deviation_seconds = sd;
            }
        }
        Ok(d)
    }

    /// `computeWeight(cutoffSeconds, travelTimeSeconds)`.
    pub fn weight(&self, cutoff_seconds: i32, travel_time_seconds: i32) -> f64 {
        let t = travel_time_seconds;
        match self.function {
            // StepDecayFunction.computeWeight
            DecayFunction::Step => {
                if t < cutoff_seconds {
                    1.0
                } else {
                    0.0
                }
            }
            // ExponentialDecayFunction.computeWeight:
            // FastMath.exp(logOneHalf / cutoffSeconds * travelTimeSeconds)
            DecayFunction::Exponential => {
                if t <= 0 {
                    1.0
                } else if t >= TWO_HOURS_IN_SECONDS {
                    0.0
                } else {
                    fastmath::exp(LOG_ONE_HALF / f64::from(cutoff_seconds) * f64::from(t))
                }
            }
            // FixedExponentialDecayFunction.computeWeight:
            // FastMath.exp(-decayConstant * travelTimeSeconds)
            DecayFunction::FixedExponential { decay_constant } => {
                if t <= 0 {
                    1.0
                } else if t >= TWO_HOURS_IN_SECONDS {
                    0.0
                } else {
                    fastmath::exp(-decay_constant * f64::from(t))
                }
            }
            // LinearDecayFunction.computeWeight (int arithmetic, wrapping
            // like Java's)
            DecayFunction::Linear { .. } => {
                let decay_begins = cutoff_seconds.wrapping_sub(self.half_width_seconds);
                let index = t.wrapping_sub(decay_begins);
                if index < 0 {
                    1.0
                } else if index < self.width_seconds {
                    self.weight_table[index as usize]
                } else {
                    0.0
                }
            }
            // LogisticDecayFunction.computeWeight = g(cutoff, 0) / g(cutoff, t)
            DecayFunction::Logistic { .. } => self.g(cutoff_seconds, 0) / self.g(cutoff_seconds, t),
        }
    }

    /// `LogisticDecayFunction.g`:
    /// `1 + exp(((travelTime - median) * PI) / (standardDeviationSeconds * SQRT3))`,
    /// with the subtraction in `int`.
    fn g(&self, median: i32, travel_time: i32) -> f64 {
        1.0 + fastmath::exp(
            (f64::from(travel_time.wrapping_sub(median)) * std::f64::consts::PI)
                / (self.standard_deviation_seconds * sqrt3()),
        )
    }

    /// `reachesZeroAt(cutoffSeconds)`: the travel time in seconds from which
    /// the weight is (treated as) zero. Step and linear have closed forms;
    /// the others use `DecayFunction.findZeroPoint`, a bisection over
    /// `[0, 4 h]` for the first time whose weight is below 0.001.
    pub fn reaches_zero_at(&self, cutoff_seconds: i32) -> i32 {
        match self.function {
            DecayFunction::Step => cutoff_seconds,
            DecayFunction::Linear { .. } => cutoff_seconds.wrapping_add(self.half_width_seconds),
            _ => {
                let (mut low, mut high) = (0i32, FOUR_HOURS_IN_SECONDS);
                while low < high {
                    let mid = (low + high) / 2;
                    // `weight < ZERO_EPSILON` is `dcmpg; ifge`: NaN is not below.
                    if self.weight(cutoff_seconds, mid) < ZERO_EPSILON {
                        high = mid;
                    } else {
                        low = mid + 1;
                    }
                }
                low
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<(&'static str, Decay)> {
        vec![
            ("step", Decay::new(DecayFunction::Step).unwrap()),
            (
                "exponential",
                Decay::new(DecayFunction::Exponential).unwrap(),
            ),
            (
                "fixed_exponential",
                Decay::new(DecayFunction::FixedExponential {
                    decay_constant: 0.001,
                })
                .unwrap(),
            ),
            (
                "linear",
                Decay::new(DecayFunction::Linear { width_minutes: 13 }).unwrap(),
            ),
            (
                "logistic",
                Decay::new(DecayFunction::Logistic {
                    standard_deviation_minutes: 10.0,
                })
                .unwrap(),
            ),
            (
                "logistic_odd",
                Decay::new(DecayFunction::Logistic {
                    standard_deviation_minutes: 1.37,
                })
                .unwrap(),
            ),
        ]
    }

    const CUTOFFS: [i32; 6] = [0, 60, 1800, 2700, 3600, 7200];

    #[test]
    fn constants_match_the_jar() {
        assert_eq!(LOG_ONE_HALF.to_bits(), 0xbfe6_2e42_fefa_39ef);
        assert_eq!(sqrt3().to_bits(), 0x3ffb_b67a_e858_4caa);
    }

    /// Zero points from the R5 jar (harness calling `reachesZeroAt`).
    #[test]
    fn zero_points_match_r5() {
        let want: [[i32; 6]; 6] = [
            [0, 60, 1800, 2700, 3600, 7200],
            [1, 598, 7200, 7200, 7200, 7200],
            [6908; 6],
            [390, 450, 2190, 3090, 3990, 7590],
            [2515, 2546, 4087, 4985, 5885, 9485],
            [345, 384, 2114, 3014, 3914, 7514],
        ];
        for (k, (name, d)) in all().iter().enumerate() {
            for (c, &cut) in CUTOFFS.iter().enumerate() {
                assert_eq!(d.reaches_zero_at(cut), want[k][c], "{name} cutoff {cut}");
            }
        }
    }

    /// Every weight for t in 0..=9000 s at each cutoff, hashed in the
    /// harness's order (`h = h * 31 + doubleToRawLongBits(w)`), against the
    /// values printed by the R5 jar after each (function, cutoff).
    #[test]
    fn weights_are_bit_identical_to_r5() {
        let want: [[i64; 6]; 6] = [
            [
                0,
                4323455642275676160,
                5476377146882523136,
                -4611686018427387904,
                5764607523034234880,
                -3458764513820540928,
            ],
            [
                -4616189618054758400,
                5010052250099082596,
                -5845750591646675017,
                -7399425113893095857,
                4050138301889688744,
                4090708391882259686,
            ],
            [
                -6872444358466683318,
                818177101997628774,
                4974464093885296074,
                43851171772079590,
                -6308403573000356534,
                -8683661575661633946,
            ],
            [
                -4898631150597285794,
                -4897434426456641642,
                -3188902411695207314,
                2952318728098268310,
                -7631370800590840978,
                7767736346523492246,
            ],
            [
                -1760459768753425756,
                -7257220623041801017,
                -6248289050123898127,
                2571577219611814832,
                4909401781895710777,
                1458738585729535851,
            ],
            [
                8212132679959203476,
                -320297031605669151,
                -8232252128965644624,
                -1378637094941983607,
                -1468158968389443745,
                -4208765392073684294,
            ],
        ];
        let mut h = 0i64;
        for (k, (name, d)) in all().iter().enumerate() {
            for (c, &cut) in CUTOFFS.iter().enumerate() {
                for t in 0..=9000 {
                    h = h
                        .wrapping_mul(31)
                        .wrapping_add(d.weight(cut, t).to_bits() as i64);
                }
                assert_eq!(h, want[k][c], "{name} cutoff {cut}");
            }
        }
    }

    #[test]
    fn step_and_linear_shapes() {
        let s = Decay::new(DecayFunction::Step).unwrap();
        assert_eq!(s.weight(1800, 1799), 1.0);
        assert_eq!(s.weight(1800, 1800), 0.0);
        let l = Decay::new(DecayFunction::Linear { width_minutes: 10 }).unwrap();
        assert_eq!(l.weight(1800, 1499), 1.0);
        assert_eq!(l.weight(1800, 1500), 1.0 - 0.5 / 600.0);
        assert_eq!(l.weight(1800, 2099), 1.0 - 599.5 / 600.0);
        assert_eq!(l.weight(1800, 2100), 0.0);
        assert_eq!(l.reaches_zero_at(1800), 2100);
        // width 0 is a step at the cutoff
        let l0 = Decay::new(DecayFunction::Linear { width_minutes: 0 }).unwrap();
        assert_eq!(l0.weight(1800, 1799), 1.0);
        assert_eq!(l0.weight(1800, 1800), 0.0);
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        for f in [
            DecayFunction::FixedExponential {
                decay_constant: 0.0,
            },
            DecayFunction::FixedExponential {
                decay_constant: 1.0,
            },
            DecayFunction::FixedExponential {
                decay_constant: f64::NAN,
            },
            DecayFunction::Linear { width_minutes: -1 },
            DecayFunction::Linear { width_minutes: 60 },
            DecayFunction::Logistic {
                standard_deviation_minutes: 0.0,
            },
            DecayFunction::Logistic {
                standard_deviation_minutes: 120.0,
            },
            DecayFunction::Logistic {
                standard_deviation_minutes: f64::NAN,
            },
            DecayFunction::Logistic {
                standard_deviation_minutes: f64::INFINITY,
            },
        ] {
            assert!(Decay::new(f.clone()).is_err(), "{f:?}");
        }
    }
}
