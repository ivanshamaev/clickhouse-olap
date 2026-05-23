import { AxisMember, Cell, MeasureMetadata, PivotResponse } from "../api/types";

// Column widths
const COL_DIM_WIDTH = 16;
const COL_DATA_WIDTH = 14;
const COL_TOTAL_WIDTH = 15;

export class ResultRenderer {
  async render(response: PivotResponse): Promise<void> {
    await Excel.run(async (ctx) => {
      const sheet = ctx.workbook.worksheets.getActiveWorksheet();

      const rowDimCount = response.row_axis.fields.length;
      const colCount = Math.max(response.column_axis.members.length, 1);
      const measureCount = response.measures.length;
      const rowCount = response.row_axis.members.length;

      // Total grid size: dimCols + (colCount * measureCount) + measureCount (grand total)
      const dataCols = colCount * measureCount;
      const totalCols = rowDimCount + dataCols + (response.column_axis.members.length > 0 ? measureCount : 0);
      const totalRows = 2 + rowCount; // 2 header rows + data rows

      // Clear old content
      const clearRange = sheet.getRangeByIndexes(0, 0, totalRows + 5, totalCols + 5);
      clearRange.clear(Excel.ClearApplyTo.all);

      // ── Header row 1: column dimension values ─────────────────────────────
      if (response.column_axis.members.length > 0) {
        response.column_axis.members.forEach((member, ci) => {
          const label = memberLabel(member);
          const startCol = rowDimCount + ci * measureCount;
          const headerCell = sheet.getRangeByIndexes(0, startCol, 1, measureCount);
          headerCell.merge(measureCount > 1);
          headerCell.values = [[label]];
          styleHeader(headerCell, false);
        });

        if (response.column_axis.members.length > 0) {
          const grandTotalStartCol = rowDimCount + dataCols;
          const gtCell = sheet.getRangeByIndexes(0, grandTotalStartCol, 1, measureCount);
          gtCell.merge(measureCount > 1);
          gtCell.values = [["Total"]];
          styleHeader(gtCell, true);
        }
      }

      // ── Header row 2: dimension field names + measure names ───────────────
      response.row_axis.fields.forEach((field, fi) => {
        const cell = sheet.getRangeByIndexes(1, fi, 1, 1);
        cell.values = [[field]];
        styleHeader(cell, false);
      });

      const measureHeaders: string[] = [];
      for (let ci = 0; ci < Math.max(colCount, 1); ci++) {
        response.measures.forEach((m) => measureHeaders.push(m.id));
      }
      if (response.column_axis.members.length > 0) {
        response.measures.forEach((m) => measureHeaders.push(m.id + " Total"));
      }
      measureHeaders.forEach((h, i) => {
        const cell = sheet.getRangeByIndexes(1, rowDimCount + i, 1, 1);
        cell.values = [[h]];
        styleHeader(cell, false);
      });

      // ── Data rows ─────────────────────────────────────────────────────────
      response.row_axis.members.forEach((member, ri) => {
        const isTotal = member.is_grand_total || member.is_subtotal;
        const rowIndex = 2 + ri;

        // Dimension cells
        const displayKey: (string | number | null)[] = member.is_grand_total
          ? ["Grand Total", ...new Array<string>(rowDimCount - 1).fill("")]
          : member.key;
        displayKey.forEach((val, fi) => {
          if (fi >= rowDimCount) return;
          const cell = sheet.getRangeByIndexes(rowIndex, fi, 1, 1);
          cell.values = [[val ?? ""]];
          if (isTotal) styleTotalRow(cell);
        });

        // Value cells
        const effectiveCols = Math.max(response.column_axis.members.length, 1);
        for (let ci = 0; ci < effectiveCols; ci++) {
          response.measures.forEach((m, mi) => {
            const colIndex = rowDimCount + ci * measureCount + mi;
            const cellVal = findCellValue(response.cells, ri, ci, m.id);
            const excelCell = sheet.getRangeByIndexes(rowIndex, colIndex, 1, 1);
            excelCell.values = [[cellVal !== null ? cellVal : ""]];
            applyNumberFormat(excelCell, m);
            if (isTotal) styleTotalRow(excelCell);
          });
        }

        // Grand total column (when column axis exists)
        if (response.column_axis.members.length > 0) {
          response.measures.forEach((m, mi) => {
            const colIndex = rowDimCount + dataCols + mi;
            // Sum across columns for additive measures; for non-additive this will be filled by engine
            const total = response.cells
              .filter((c) => c.r === ri && c.m === m.id)
              .reduce((sum, c) => (sum ?? 0) + (c.v ?? 0), null as number | null);
            const excelCell = sheet.getRangeByIndexes(rowIndex, colIndex, 1, 1);
            excelCell.values = [[total !== null ? total : ""]];
            applyNumberFormat(excelCell, m);
            if (isTotal) styleTotalRow(excelCell);
          });
        }
      });

      // ── Column widths ─────────────────────────────────────────────────────
      for (let i = 0; i < rowDimCount; i++) {
        sheet.getRangeByIndexes(0, i, totalRows, 1).format.columnWidth = COL_DIM_WIDTH * 7;
      }
      for (let i = rowDimCount; i < totalCols; i++) {
        const isGrandTotal = i >= rowDimCount + dataCols;
        sheet.getRangeByIndexes(0, i, totalRows, 1).format.columnWidth =
          (isGrandTotal ? COL_TOTAL_WIDTH : COL_DATA_WIDTH) * 7;
      }

      await ctx.sync();
    });
  }
}

function memberLabel(member: AxisMember): string {
  if (member.is_grand_total) return "Grand Total";
  return member.key.filter((k) => k !== null && k !== "__TOTAL__" && k !== "__GRAND_TOTAL__").join(" / ");
}

function findCellValue(cells: Cell[], r: number, c: number, measureId: string): number | null {
  return cells.find((cell) => cell.r === r && cell.c === c && cell.m === measureId)?.v ?? null;
}

function styleHeader(range: Excel.Range, isTotal: boolean): void {
  range.format.fill.color = isTotal ? "#1a3a5c" : "#2B579A";
  range.format.font.color = "#FFFFFF";
  range.format.font.bold = true;
  range.format.horizontalAlignment = "Center";
}

function styleTotalRow(range: Excel.Range): void {
  range.format.fill.color = "#E8F0FD";
  range.format.font.bold = true;
}

function applyNumberFormat(range: Excel.Range, measure: MeasureMetadata): void {
  switch (measure.format) {
    case "currency":
      range.numberFormat = [["#,##0.00"]];
      break;
    case "integer":
      range.numberFormat = [["#,##0"]];
      break;
    case "percent":
      range.numberFormat = [["0.00%"]];
      break;
  }
}
