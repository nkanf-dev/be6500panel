import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "../theme";
import { CommandPalette } from "./command-palette";
it("filters command registry and navigates with the keyboard", async () => {
  const navigate = vi.fn(),
    setOpen = vi.fn();
  const user = userEvent.setup();
  render(
    <ThemeProvider>
      <CommandPalette open setOpen={setOpen} navigate={navigate} />
    </ThemeProvider>,
  );
  await user.type(screen.getByRole("combobox"), "frpc");
  expect(screen.getAllByRole("option")).toHaveLength(1);
  await user.keyboard("{Enter}");
  expect(navigate).toHaveBeenCalledWith("frpc");
  expect(setOpen).toHaveBeenCalledWith(false);
});
