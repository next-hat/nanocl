use crate::models::{HttpMetricDb, StreamMetricDb};

use super::generic::*;

impl RepositoryBase for HttpMetricDb {}

impl RepositoryCreate for HttpMetricDb {}

impl RepositoryBase for StreamMetricDb {}

impl RepositoryCreate for StreamMetricDb {}
