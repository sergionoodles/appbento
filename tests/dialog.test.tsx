import { useLayoutEffect, useState } from "react";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Dialog } from "../src/components/dialogs/dialog";

function Example() {
  const [isOpen, setIsOpen] = useState(false);
  return <>
    <button onClick={() => setIsOpen(true)}>Open folders</button>
    {isOpen ? <Dialog title="Folders" onClose={() => setIsOpen(false)}>
      <input aria-label="Folder" /><button onClick={() => setIsOpen(false)}>Close</button>
    </Dialog> : null}
  </>;
}

describe("accessible dialog", () => {
  it("moves focus inside, traps Tab in both directions and restores focus on Escape", async () => {
    const user = userEvent.setup();
    render(<Example />);
    const opener = screen.getByRole("button", { name: "Open folders" });
    await user.click(opener);
    const folder = screen.getByRole("textbox", { name: "Folder" });
    const close = screen.getByRole("button", { name: "Close" });
    expect(folder).toHaveFocus();
    await user.tab({ shift: true });
    expect(close).toHaveFocus();
    await user.tab();
    expect(folder).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });
});

function ClearFocusOnMount() {
  useLayoutEffect(() => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  }, []);
  return null;
}

function InertFocusExample() {
  const [isOpen, setIsOpen] = useState(false);
  const [trigger, setTrigger] = useState<HTMLElement | null>(null);
  return <>
    <button onClick={(event) => {
      setTrigger(event.currentTarget);
      setIsOpen(true);
    }}>Open folders</button>
    {isOpen ? <>
      <ClearFocusOnMount />
      <Dialog title="Folders" onClose={() => setIsOpen(false)} {...{ returnFocusElement: trigger }}>
        <input aria-label="Folder" />
      </Dialog>
    </> : null}
  </>;
}

it("restores the captured trigger when opening a modal has cleared document focus", async () => {
  const user = userEvent.setup();
  render(<InertFocusExample />);
  const opener = screen.getByRole("button", { name: "Open folders" });
  await user.click(opener);
  expect(screen.getByRole("textbox", { name: "Folder" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(opener).toHaveFocus();
});
