import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SqlCellEditor } from "./index";

const editableCell = {
  resultIndex: 0,
  rowIndex: 0,
  columnName: "task_name",
  columnType: "varchar",
  tableName: "work_order",
  primaryKey: { id: "1" },
  oldValue: "旧值",
};

describe("SqlCellEditor", () => {
  afterEach(() => cleanup());

  it("输入草稿时不提交，Enter 和后续失焦只提交一次", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn(() => true);

    render(
      <SqlCellEditor
        cell={editableCell}
        saving={false}
        onSave={onSave}
        onCancel={vi.fn()}
      />,
    );

    const input = screen.getByRole("textbox", { name: "编辑 task_name" });
    await user.clear(input);
    await user.type(input, "新值");
    expect(onSave).not.toHaveBeenCalled();

    await user.keyboard("{Enter}");
    await user.tab();

    expect(onSave).toHaveBeenCalledOnce();
    expect(onSave).toHaveBeenCalledWith("新值");
  });
});
