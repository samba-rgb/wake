use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// A single data point in a time series
#[derive(Debug, Clone)]
pub struct TimeSeriesPoint<T> {
    pub timestamp: DateTime<Utc>,
    pub value: T,
}

/// A generic time series of values
#[derive(Debug, Clone)]
pub struct TimeSeries<T> {
    pub name: String,
    pub points: VecDeque<TimeSeriesPoint<T>>,
    pub max_points: usize,
}

/// AsRef trait for accessing values
pub trait AsRef<T> {
    fn as_ref(&self) -> &T;
}

impl<T> AsRef<T> for T {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: Clone> TimeSeries<T> {
    /// Create a new time series with a name and maximum number of points
    pub fn new(name: &str, max_points: usize) -> Self {
        Self {
            name: name.to_string(),
            points: VecDeque::with_capacity(max_points),
            max_points,
        }
    }
    
    /// Add a data point to the time series
    pub fn add_point(&mut self, timestamp: DateTime<Utc>, value: T) {
        self.points.push_back(TimeSeriesPoint {
            timestamp,
            value,
        });
        
        // Remove oldest points if we exceed the maximum
        while self.points.len() > self.max_points {
            self.points.pop_front();
        }
    }
    
    /// Get the latest data point
    pub fn latest(&self) -> Option<&TimeSeriesPoint<T>> {
        self.points.back()
    }
    
    /// Get the earliest data point
    pub fn earliest(&self) -> Option<&TimeSeriesPoint<T>> {
        self.points.front()
    }
    
    /// Get the number of points in the time series
    pub fn len(&self) -> usize {
        self.points.len()
    }
    
    /// Check if the time series is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}