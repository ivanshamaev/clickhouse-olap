#!/usr/bin/env bash
# Seed ClickHouse with a test sales dataset used by integration tests.
set -euo pipefail

CH_URL="${CLICKHOUSE_URL:-http://localhost:8123}"
DB="default"

echo "Seeding test data into $CH_URL/$DB ..."

clickhouse_query() {
    curl -s --fail -X POST "$CH_URL/?database=$DB" --data-binary "$1"
}

# ── Create table ──────────────────────────────────────────────────────────────
clickhouse_query "
CREATE TABLE IF NOT EXISTS sales (
    order_id         UInt64,
    customer_id      UInt64,
    order_date       Date,
    region           LowCardinality(String),
    country          LowCardinality(String),
    product_category LowCardinality(String),
    amount           Decimal(18, 2),
    qty              UInt32
) ENGINE = MergeTree()
ORDER BY (region, order_date, order_id)
"
echo "Table 'sales' ready."

# ── Insert test rows ──────────────────────────────────────────────────────────
clickhouse_query "
INSERT INTO sales VALUES
  (1,  101, '2025-01-05', 'Baltics', 'LV', 'Electronics',  1250.00, 2),
  (2,  102, '2025-01-12', 'Baltics', 'LT', 'Electronics',   890.50, 1),
  (3,  103, '2025-01-20', 'Baltics', 'EE', 'Clothing',      320.00, 3),
  (4,  104, '2025-02-03', 'Baltics', 'LV', 'Electronics',  2100.00, 1),
  (5,  105, '2025-02-14', 'Baltics', 'LT', 'Clothing',      450.00, 4),
  (6,  106, '2025-02-28', 'Baltics', 'EE', 'Electronics',  1800.00, 2),
  (7,  107, '2025-03-08', 'Baltics', 'LV', 'Food',          180.00, 10),
  (8,  108, '2025-03-15', 'Baltics', 'LT', 'Food',          220.00, 8),
  (9,  109, '2025-03-22', 'Baltics', 'EE', 'Clothing',      670.00, 5),
  (10, 110, '2025-01-08', 'Nordics', 'SE', 'Electronics',  3400.00, 3),
  (11, 111, '2025-01-18', 'Nordics', 'NO', 'Electronics',  4200.00, 2),
  (12, 112, '2025-02-10', 'Nordics', 'SE', 'Clothing',      980.00, 6),
  (13, 113, '2025-02-20', 'Nordics', 'NO', 'Food',          310.00, 12),
  (14, 114, '2025-03-05', 'Nordics', 'SE', 'Food',          260.00, 9),
  (15, 115, '2025-03-18', 'Nordics', 'NO', 'Clothing',     1100.00, 7)
"
echo "Inserted 15 test rows."

# ── Verify ────────────────────────────────────────────────────────────────────
COUNT=$(clickhouse_query "SELECT count() FROM sales FORMAT TSV")
echo "Total rows in sales: $COUNT"
echo "Done."
