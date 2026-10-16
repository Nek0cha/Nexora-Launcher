import { useEffect, useMemo, useState } from "react";
import { Autocomplete, Dialog, Input, Menu, MenuItem, Modal, ModalOverlay, SearchField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import IconSearch from "~icons/ph/magnifying-glass";
import { filterCommands, modifierLabel, nextTheme, type PaletteCommand } from "../features/palette/commands";
import { useUpdateSetting } from "../features/settings/useSettings";

const PAGES = [
  { path: "/", key: "home" },
  { path: "/instances", key: "instances" },
  { path: "/browse", key: "browse" },
  { path: "/accounts", key: "accounts" },
  { path: "/settings", key: "settings" },
] as const;

// 背景の暗幕は不透明度だけを変える
const overlayClass = [
  "fixed inset-0 z-50 flex items-start justify-center bg-surface-0/60 p-4 pt-[15vh]",
  "transition-opacity duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 data-exiting:opacity-0 data-exiting:duration-(--duration-fast)",
].join(" ");

const modalClass = [
  "w-full max-w-lg rounded-lg border border-line bg-surface-2 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "data-entering:opacity-0 motion-safe:data-entering:translate-y-2",
  "data-exiting:opacity-0 motion-safe:data-exiting:translate-y-2 data-exiting:duration-(--duration-fast)",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

export function CommandPalette() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const update = useUpdateSetting();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const close = () => {
    setOpen(false);
    setQuery("");
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const commands = useMemo<PaletteCommand[]>(
    () => [
      ...PAGES.map((p) => ({
        id: `go-${p.key}`,
        label: t("palette.goTo", { page: t(`nav.${p.key}`) }),
        run: () => void navigate(p.path),
      })),
      {
        id: "toggle-theme",
        label: t("palette.toggleTheme"),
        run: () => {
          const current = document.documentElement.dataset.theme === "light" ? "light" : "dark";
          // 失敗は設定画面の書き込みエラーと共有される（useSettingsWriteError）
          update.mutate({ key: "theme", value: nextTheme(current) });
        },
      },
    ],
    [t, navigate, update],
  );

  const visible = filterCommands(commands, query);
  const mod = modifierLabel(navigator.platform);

  return (
    <ModalOverlay
      isOpen={open}
      onOpenChange={(v) => {
        if (v) setOpen(true);
        else close();
      }}
      isDismissable
      className={overlayClass}
    >
      <Modal className={modalClass}>
        <Dialog aria-label={t("palette.placeholder")} className="outline-hidden">
          {/* 絞り込みは filterCommands に一本化する。Autocomplete 側の絞り込みは何も除かない */}
          <Autocomplete inputValue={query} onInputChange={setQuery} filter={() => true}>
            <SearchField
              aria-label={t("palette.placeholder")}
              autoFocus
              className="flex items-center gap-2 border-b border-line px-3"
            >
              <IconSearch aria-hidden className="size-4 shrink-0 text-text-muted" />
              <Input
                placeholder={t("palette.placeholder")}
                className="h-11 min-w-0 flex-1 bg-transparent text-text outline-hidden placeholder:text-text-muted [&::-webkit-search-cancel-button]:hidden"
              />
              <kbd className="tabular shrink-0 rounded-sm border border-line px-1.5 text-xs text-text-muted">
                {mod === "⌘" ? "⌘K" : "Ctrl+K"}
              </kbd>
            </SearchField>
            <Menu
              items={visible}
              aria-label={t("palette.placeholder")}
              renderEmptyState={() => <p className="px-3 py-4 text-text-muted">{t("palette.noResults")}</p>}
              className="max-h-80 overflow-auto p-1 outline-hidden"
              onAction={(key) => {
                // 先に閉じてから実行する（ページ移動などで閉じる動きと重ならないように）
                const command = commands.find((c) => c.id === key);
                close();
                command?.run();
              }}
            >
              {(item) => (
                <MenuItem
                  id={item.id}
                  textValue={item.label}
                  className="flex h-10 cursor-default items-center rounded-sm px-3 text-text outline-hidden data-focused:bg-surface-3"
                >
                  {item.label}
                </MenuItem>
              )}
            </Menu>
          </Autocomplete>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
