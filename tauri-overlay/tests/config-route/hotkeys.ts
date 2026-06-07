import { test, expect } from "@playwright/test";
import {
    expectHotkeyIdle,
    expectHotkeyRecording,
    hotkeyInputForAction,
    installTauriMock,
} from "../helpers/config-route-mock";

test.describe("Config route hotkeys", () => {
    test.describe.configure({ timeout: 60000 });

    test("reassigned hotkeys apply immediately and save only after Save is pressed", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+*",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const hotkeyInput = hotkeyInputForAction(page, /^Show \/ Hide$/);

        await hotkeyInput.click();
        await hotkeyInput.dispatchEvent("keydown", {
            key: "Control",
            code: "ControlLeft",
            ctrlKey: true,
        });

        await expectHotkeyRecording(hotkeyInput);

        await hotkeyInput.dispatchEvent("keydown", {
            key: "P",
            code: "KeyP",
            ctrlKey: true,
            shiftKey: true,
        });

        await expectHotkeyIdle(hotkeyInput);
        await expect(hotkeyInput).toHaveValue("Ctrl+Shift+P");

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+P",
            });

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests = window["__SCO_ACTION_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                action: "hotkey_reassign_end",
                payload: {
                    path: "hotkey_show/hide",
                },
            });

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+P",
            });
    });

    test("shifted symbol keys stay normalized in draft and save as base keys", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+8",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const hotkeyInput = hotkeyInputForAction(page, /^Show \/ Hide$/);

        await hotkeyInput.click();
        await expectHotkeyRecording(hotkeyInput);

        await hotkeyInput.dispatchEvent("keydown", {
            key: "&",
            code: "Digit7",
            ctrlKey: true,
            shiftKey: true,
        });

        await expect(hotkeyInput).toHaveValue("Ctrl+Shift+7");
        await expectHotkeyIdle(hotkeyInput);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+7",
            });

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+7",
            });
    });

    test("escape and backspace clear hotkey assignments in draft and Revert restores saved values", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+*",
                hotkey_show: "Ctrl+Alt+K",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const showHideInput = hotkeyInputForAction(page, /^Show \/ Hide$/);
        const showInput = hotkeyInputForAction(page, /^Show$/);

        await showHideInput.click();
        await expectHotkeyRecording(showHideInput);
        await showHideInput.dispatchEvent("keydown", {
            key: "Escape",
            code: "Escape",
        });

        await expect(showHideInput).toHaveValue("");
        await expectHotkeyIdle(showHideInput);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await showInput.click();
        await expectHotkeyRecording(showInput);
        await showInput.dispatchEvent("keydown", {
            key: "Backspace",
            code: "Backspace",
        });

        await expect(showInput).toHaveValue("");
        await expectHotkeyIdle(showInput);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await page.getByRole("button", { name: /^Revert$/ }).click();

        await expect(showHideInput).toHaveValue("Ctrl+Shift+*");
        await expect(showInput).toHaveValue("Ctrl+Alt+K");
    });
});
