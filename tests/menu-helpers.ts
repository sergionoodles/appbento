import type { UserEvent } from "@testing-library/user-event";
import { screen } from "@testing-library/react";

export async function chooseFromMenu(user: UserEvent, menu: string, item: string | RegExp) {
  await user.click(screen.getByRole("button", { name: menu }));
  await user.click(await screen.findByRole("menuitem", { name: item }));
}

export async function chooseOption(user: UserEvent, combobox: HTMLElement, option: string) {
  await user.click(combobox);
  await user.click(screen.getByRole("option", { name: option }));
}
