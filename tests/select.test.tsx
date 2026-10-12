import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Dialog } from "../src/components/dialogs/dialog";
import { Select } from "../src/components/ui/select";

const CHANNELS = [
  { value: "stable", label: "Stable" },
  { value: "beta", label: "Beta" },
  { value: "nightly", label: "Nightly" },
] as const;

function ChannelPicker({ onClose = () => undefined }: { onClose?: () => void }) {
  const [channel, setChannel] = useState<(typeof CHANNELS)[number]["value"]>("stable");
  return (
    <Dialog title="Channel" onClose={onClose}>
      <Select aria-label="Channel" value={channel} options={CHANNELS} onChange={setChannel} />
    </Dialog>
  );
}

describe("Select", () => {
  it("chooses an option with the mouse and marks it selected", async () => {
    const user = userEvent.setup();
    render(<ChannelPicker />);
    await user.click(screen.getByRole("combobox", { name: "Channel" }));
    await user.click(screen.getByRole("option", { name: "Beta" }));
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Channel" })).toHaveTextContent("Beta");
    await user.click(screen.getByRole("combobox", { name: "Channel" }));
    expect(screen.getByRole("option", { name: "Beta" })).toHaveAttribute("aria-selected", "true");
  });

  it("chooses an option with the keyboard", async () => {
    const user = userEvent.setup();
    render(<ChannelPicker />);
    screen.getByRole("combobox", { name: "Channel" }).focus();
    await user.keyboard("{ArrowDown}{End}{Enter}");
    expect(screen.getByRole("combobox", { name: "Channel" })).toHaveTextContent("Nightly");
  });

  it("closes only the list on Escape, keeping the dialog open", async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<ChannelPicker onClose={onClose} />);
    await user.click(screen.getByRole("combobox", { name: "Channel" }));
    await user.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Channel" })).toHaveTextContent("Stable");
    expect(onClose).not.toHaveBeenCalled();
  });
});
