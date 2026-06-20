import { describe, it, expect } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/ToasterHarness.svelte";
import { Ui } from "../state/ui.svelte";

const MESSAGE = "Couldn't save your comment — your draft is kept.";

describe("Toaster", () => {
  it("renders an error toast with the alert role and message", () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: MESSAGE });
    const { getByRole } = render(Harness, { ui });
    const alert = getByRole("alert");
    expect(alert.textContent).toContain(MESSAGE);
  });

  it("dismisses a toast when its close button is clicked", async () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: MESSAGE });
    const { getByRole, queryByText } = render(Harness, { ui });
    await fireEvent.click(getByRole("button", { name: "Dismiss" }));
    expect(queryByText(MESSAGE)).toBeNull();
  });
});
