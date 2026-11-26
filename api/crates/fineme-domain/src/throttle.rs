//! Limitation des tentatives d'authentification : délai croissant après des échecs répétés,
//! par adresse IP et pour l'ensemble du serveur. Règles pures : l'instant est toujours fourni par l'appelant.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

/// Réglage d'un compteur d'échecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrottlePolicy {
    /// Échecs tolérés sans délai.
    pub free_failures: u32,
    /// Délai imposé au premier échec au-delà des échecs tolérés ; doublé à chaque échec suivant.
    pub base_delay: Duration,
    /// Plafond du délai.
    pub max_delay: Duration,
    /// Sans nouvel échec pendant cette durée (et une fois le délai écoulé), le compteur repart de zéro.
    pub forget_after: Duration,
}

impl ThrottlePolicy {
    /// Par adresse IP : 5 essais libres, puis 30 s, 1 min, 2 min… jusqu'à 15 min.
    pub fn per_ip() -> Self {
        Self {
            free_failures: 5,
            base_delay: Duration::seconds(30),
            max_delay: Duration::minutes(15),
            forget_after: Duration::hours(1),
        }
    }

    /// Pour tout le serveur (attaque répartie sur de nombreuses IP) : seuil plus haut, plafond plus bas
    /// pour que le propriétaire ne reste pas bloqué longtemps.
    pub fn global() -> Self {
        Self {
            free_failures: 50,
            base_delay: Duration::seconds(30),
            max_delay: Duration::minutes(5),
            forget_after: Duration::hours(1),
        }
    }

    /// Pour tout le serveur MCP : seuil très haut et plafond court, pour qu'un flot de faux jetons venu de
    /// nombreuses IP ne bloque pas longtemps le vrai client (le secret de 256 bits n'est pas devinable).
    pub fn global_lenient() -> Self {
        Self {
            free_failures: 500,
            base_delay: Duration::seconds(10),
            max_delay: Duration::minutes(1),
            forget_after: Duration::hours(1),
        }
    }

    /// Délai imposé après `failures` échecs consécutifs (aucun tant que le seuil n'est pas dépassé).
    pub fn delay_after(&self, failures: u32) -> Duration {
        if failures <= self.free_failures {
            return Duration::zero();
        }
        // 2^n plafonné : au-delà de 2^20 le plafond est de toute façon atteint.
        let doublings = (failures - self.free_failures - 1).min(20);
        let factor = 1_i32 << doublings;
        (self.base_delay * factor).min(self.max_delay)
    }
}

/// Échecs consécutifs d'une source et date de fin du délai en cours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FailureRecord {
    failures: u32,
    last_failure: DateTime<Utc>,
    blocked_until: DateTime<Utc>,
}

impl FailureRecord {
    fn is_stale(&self, policy: &ThrottlePolicy, now: DateTime<Utc>) -> bool {
        now >= self.blocked_until && now - self.last_failure >= policy.forget_after
    }

    fn remaining(&self, now: DateTime<Utc>) -> Option<Duration> {
        (now < self.blocked_until).then(|| self.blocked_until - now)
    }
}

#[derive(Debug, Clone)]
struct Counter {
    policy: ThrottlePolicy,
    record: Option<FailureRecord>,
}

impl Counter {
    fn new(policy: ThrottlePolicy) -> Self {
        Self {
            policy,
            record: None,
        }
    }

    fn remaining(&self, now: DateTime<Utc>) -> Option<Duration> {
        self.record.and_then(|record| record.remaining(now))
    }

    fn fail(&mut self, now: DateTime<Utc>) {
        let failures = match self.record {
            Some(record) if !record.is_stale(&self.policy, now) => record.failures + 1,
            _ => 1,
        };
        self.record = Some(FailureRecord {
            failures,
            last_failure: now,
            blocked_until: now + self.policy.delay_after(failures),
        });
    }
}

/// Au-delà, les sources oubliables sont purgées à chaque échec (mémoire bornée face à une attaque).
const MAX_TRACKED_SOURCES: usize = 10_000;

/// État du limiteur : un compteur par source (adresse IP) et un compteur global.
#[derive(Debug, Clone)]
pub struct LoginThrottle {
    per_source: ThrottlePolicy,
    sources: HashMap<String, FailureRecord>,
    global: Counter,
}

impl Default for LoginThrottle {
    fn default() -> Self {
        Self::new(ThrottlePolicy::per_ip(), ThrottlePolicy::global())
    }
}

impl LoginThrottle {
    pub fn new(per_source: ThrottlePolicy, global: ThrottlePolicy) -> Self {
        Self {
            per_source,
            sources: HashMap::new(),
            global: Counter::new(global),
        }
    }

    /// `Err(attente)` si une tentative de `source` doit être refusée à `now` ; l'attente est la plus longue
    /// des deux limites (source et globale).
    pub fn check(&self, source: &str, now: DateTime<Utc>) -> Result<(), Duration> {
        let by_source = self
            .sources
            .get(source)
            .and_then(|record| record.remaining(now));
        match by_source
            .into_iter()
            .chain(self.global.remaining(now))
            .max()
        {
            Some(wait) => Err(wait),
            None => Ok(()),
        }
    }

    /// Enregistre un échec de `source` à `now`.
    pub fn record_failure(&mut self, source: &str, now: DateTime<Utc>) {
        if self.sources.len() >= MAX_TRACKED_SOURCES {
            let policy = self.per_source;
            self.sources
                .retain(|_, record| !record.is_stale(&policy, now));
        }
        let mut counter = Counter {
            policy: self.per_source,
            record: self.sources.get(source).copied(),
        };
        counter.fail(now);
        if let Some(record) = counter.record {
            self.sources.insert(source.to_string(), record);
        }
        self.global.fail(now);
    }

    /// Une réussite efface les échecs de la source (le compteur global, lui, s'oublie avec le temps).
    pub fn record_success(&mut self, source: &str) {
        self.sources.remove(source);
    }
}

/// Attente arrondie à la seconde supérieure (pour l'en-tête `Retry-After`), au moins 1 s.
pub fn retry_after_seconds(wait: Duration) -> u64 {
    let millis = wait.num_milliseconds().max(1);
    u64::try_from((millis + 999) / 1000).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    fn policy(free: u32) -> ThrottlePolicy {
        ThrottlePolicy {
            free_failures: free,
            base_delay: Duration::seconds(10),
            max_delay: Duration::seconds(60),
            forget_after: Duration::minutes(30),
        }
    }

    #[test]
    fn delay_doubles_after_free_failures_and_is_capped() {
        let p = policy(3);
        let delays: Vec<i64> = (0..=8).map(|n| p.delay_after(n).num_seconds()).collect();
        assert_eq!(delays, vec![0, 0, 0, 0, 10, 20, 40, 60, 60]);
        assert_eq!(p.delay_after(u32::MAX), Duration::seconds(60));
    }

    #[test]
    fn default_policies_escalate_up_to_their_ceiling() {
        let ip = ThrottlePolicy::per_ip();
        assert_eq!(ip.delay_after(5), Duration::zero());
        assert_eq!(ip.delay_after(6), Duration::seconds(30));
        assert_eq!(ip.delay_after(7), Duration::minutes(1));
        assert_eq!(ip.delay_after(100), Duration::minutes(15));
        assert_eq!(
            ThrottlePolicy::global().delay_after(1000),
            Duration::minutes(5)
        );
    }

    #[test]
    fn a_source_is_blocked_after_too_many_failures_then_released() {
        let mut throttle = LoginThrottle::new(policy(2), policy(100));
        let now = t0();
        for _ in 0..2 {
            assert!(throttle.check("1.1.1.1", now).is_ok());
            throttle.record_failure("1.1.1.1", now);
        }
        assert!(throttle.check("1.1.1.1", now).is_ok());
        throttle.record_failure("1.1.1.1", now);
        assert_eq!(throttle.check("1.1.1.1", now), Err(Duration::seconds(10)));
        assert_eq!(
            throttle.check("1.1.1.1", now + Duration::seconds(4)),
            Err(Duration::seconds(6))
        );
        // Une autre adresse n'est pas concernée.
        assert!(throttle.check("2.2.2.2", now).is_ok());
        // Le délai écoulé, une nouvelle tentative est permise ; un nouvel échec double l'attente.
        let later = now + Duration::seconds(10);
        assert!(throttle.check("1.1.1.1", later).is_ok());
        throttle.record_failure("1.1.1.1", later);
        assert_eq!(throttle.check("1.1.1.1", later), Err(Duration::seconds(20)));
    }

    #[test]
    fn success_clears_the_source_and_old_failures_are_forgotten() {
        let mut throttle = LoginThrottle::new(policy(1), policy(100));
        let now = t0();
        throttle.record_failure("ip", now);
        throttle.record_failure("ip", now);
        assert!(throttle.check("ip", now).is_err());
        throttle.record_success("ip");
        assert!(throttle.check("ip", now).is_ok());

        throttle.record_failure("ip", now);
        throttle.record_failure("ip", now);
        // 30 min plus tard, le compteur repart de zéro : un échec isolé ne bloque plus.
        let later = now + Duration::minutes(31);
        throttle.record_failure("ip", later);
        assert!(throttle.check("ip", later).is_ok());
    }

    #[test]
    fn global_limit_blocks_every_source() {
        let mut throttle = LoginThrottle::new(policy(100), policy(3));
        let now = t0();
        for i in 0..4 {
            throttle.record_failure(&format!("10.0.0.{i}"), now);
        }
        assert_eq!(
            throttle.check("192.168.1.1", now),
            Err(Duration::seconds(10))
        );
        assert!(
            throttle
                .check("192.168.1.1", now + Duration::seconds(10))
                .is_ok()
        );
    }

    #[test]
    fn the_longest_wait_wins() {
        let mut throttle = LoginThrottle::new(policy(0), policy(1));
        let now = t0();
        throttle.record_failure("ip", now); // source : 10 s ; global : 1 échec toléré
        throttle.record_failure("ip", now); // source : 20 s ; global : 10 s
        assert_eq!(throttle.check("ip", now), Err(Duration::seconds(20)));
        assert_eq!(throttle.check("autre", now), Err(Duration::seconds(10)));
    }

    #[test]
    fn retry_after_is_rounded_up_to_the_second() {
        assert_eq!(retry_after_seconds(Duration::milliseconds(1)), 1);
        assert_eq!(retry_after_seconds(Duration::milliseconds(1001)), 2);
        assert_eq!(retry_after_seconds(Duration::seconds(30)), 30);
        assert_eq!(retry_after_seconds(Duration::zero()), 1);
    }
}
