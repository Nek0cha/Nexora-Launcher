import { useRef, useState } from "react";
import { Button, Input, Menu, MenuItem, MenuSection, MenuTrigger, Popover, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconDots from "~icons/ph/dots-three";
import { IpcError } from "../../api/ipc";
import type { SavedSkin } from "../../bindings/SavedSkin";
import type { SkinVariant } from "../../bindings/SkinVariant";
import { MAX_SKIN_BYTES, type useSkinActions } from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 小さな丸いボタン。カードに乗せたときとフォーカスしたとき、開いているときだけ見える（大きさは常に同じ）
const menuButton = [
  "pointer-events-auto absolute top-1.5 left-1.5 flex size-7 items-center justify-center rounded-md",
  "bg-surface-1/80 text-text-muted opacity-0",
  "transition-[opacity,background-color,color] duration-(--duration-fast) ease-out-quint",
  "group-hover:opacity-100 group-focus-within:opacity-100 aria-expanded:opacity-100",
  "hover:bg-surface-3 hover:text-text pressed:bg-surface-3",
  // 押せないときも見える条件は同じで、見えるときだけ薄くする（取り直しの間に全カードへ点滅しない）
  "disabled:opacity-0 group-hover:disabled:opacity-50 group-focus-within:disabled:opacity-50",
  ring,
].join(" ");

// 名前の入力はカードの下端に重ねる（カードの大きさは変えない）
const nameInput = [
  "pointer-events-auto h-8 w-full min-w-0 rounded-md border border-line-control bg-surface-1 px-2 text-sm text-text",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

const popoverClass = [
  "min-w-44 max-w-72 rounded-lg border border-line bg-surface-2 p-1 shadow-float",
  "transition-[opacity,translate] duration-(--duration-base) ease-out-quint",
  "entering:opacity-0 exiting:opacity-0 exiting:duration-(--duration-fast)",
  "motion-safe:placement-bottom:entering:-translate-y-1 motion-safe:placement-bottom:exiting:-translate-y-1",
  "motion-safe:placement-top:entering:translate-y-1 motion-safe:placement-top:exiting:translate-y-1",
  "motion-reduce:transition-opacity motion-reduce:duration-(--duration-fast)",
].join(" ");

const menuItemClass = [
  "flex h-9 cursor-default items-center gap-2 rounded-sm px-2 text-sm text-text outline-none",
  "hover:bg-surface-3 focused:bg-surface-3",
].join(" ");

const dangerItemClass = `${menuItemClass} text-danger`;

type Actions = Pick<ReturnType<typeof useSkinActions>, "update" | "remove" | "apply">;
type Patch = { name?: string; variant?: SkinVariant; png?: number[] };

/**
 * 保存したスキン 1 枚のメニュー（名前を変える、classic / slim、ファイルを差し替える、削除）。
 * 操作の組はスキン画面と共通のものを受け取る（取り直しが終わるまでの「進行中」を 1 つにするため）。
 * 着ているスキンを変えたときだけ、更新のあとに適用し直して Mojang 側へ反映する。削除はライブラリだけで、確認は挟まない。
 */
export function SkinMenu({ skin, worn, canApply, actions, disabled, onWorking }: {
  skin: SavedSkin;
  /** いま着ているスキンか */
  worn: boolean;
  canApply: boolean;
  actions: Actions;
  /** 適用・追加など、ほかの操作の最中 */
  disabled: boolean;
  /** 読み込み・更新・適用の一連の最中は真。親が全てのカードとメニューを止めるのに使う */
  onWorking: (working: boolean) => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const picker = useRef<HTMLInputElement>(null);
  // 送った名前（または Esc で取り消した編集）。Enter や Esc の後にフォーカスが外れても、名前を送らない
  const sent = useRef(false);
  const blocked = disabled || actions.update.isPending || actions.remove.isPending;

  /**
   * 更新し、着ているスキンの見た目が変わるとき（モデルの切り替えとファイルの差し替え）だけ適用し直す。名前だけの変更は
   * 手元の記録だけで、Mojang へ送り直さない。失敗はトーストで知らせ、更新が失敗したら適用しない。
   * 内容を作る処理（ファイルの読み込み）から取り直しの終わりまで、画面全体の操作を止める（onWorking）。
   */
  const run = async (build: () => Promise<Patch> | Patch) => {
    onWorking(true);
    try {
      const patch = await build();
      await actions.update.mutateAsync({
        skinId: skin.id,
        patch: { name: patch.name ?? null, variant: patch.variant ?? null, png: patch.png ?? null },
      });
      if (worn && canApply && (patch.variant !== undefined || patch.png !== undefined)) await actions.apply.mutateAsync({ skinId: skin.id });
    } catch (e) {
      toast.showError(e);
    } finally {
      onWorking(false);
    }
  };

  const commitName = async () => {
    if (sent.current) return;
    const name = draft.trim();
    if (name === "" || name === skin.name) {
      setEditing(false);
      return;
    }
    sent.current = true;
    // 保存と取り直しが終わるまで入力を残す（古い名前に一瞬戻らない）
    await run(() => ({ name }));
    sent.current = false;
    setEditing(false);
  };

  const onPicked = (files: FileList | null) => {
    const file = files?.[0];
    // 同じファイルをもう一度選べるように空にする
    if (picker.current) picker.current.value = "";
    if (!file || blocked) return;
    void run(async () => {
      // 読む前に大きさを見る。本体の検査と同じ失敗にする
      if (file.size > MAX_SKIN_BYTES) throw new IpcError({ code: "skin.invalid", params: {}, detail: null });
      return { png: Array.from(new Uint8Array(await file.arrayBuffer())) };
    });
  };

  const onAction = (key: string | number) => {
    switch (key) {
      case "rename":
        sent.current = false;
        setDraft(skin.name);
        setEditing(true);
        break;
      case "classic":
      case "slim":
        if (key !== skin.variant) void run(() => ({ variant: key }));
        break;
      case "replace":
        picker.current?.click();
        break;
      case "delete":
        actions.remove.mutate({ skinId: skin.id }, { onError: (e) => toast.showError(e) });
        break;
    }
  };

  const variants: SkinVariant[] = ["classic", "slim"];

  return (
    <>
      <MenuTrigger>
        <Button aria-label={t("skins.menu")} isDisabled={blocked} className={menuButton}>
          <IconDots aria-hidden className="size-5" />
        </Button>
        <Popover placement="bottom start" offset={4} className={popoverClass}>
          <Menu aria-label={t("skins.menu")} className="outline-none">
            <MenuItem id="rename" onAction={() => onAction("rename")} className={menuItemClass}>
              <span className="size-4" />
              {t("skins.rename")}
            </MenuItem>
            <MenuSection aria-label={t("skins.model")} selectionMode="single" selectedKeys={[skin.variant]}>
              {variants.map((v) => (
                <MenuItem key={v} id={v} onAction={() => onAction(v)} className={menuItemClass}>
                  {({ isSelected }) => (
                    <>
                      <IconCheck aria-hidden className={`size-4 ${isSelected ? "" : "invisible"}`} />
                      {t(`skins.${v}`)}
                    </>
                  )}
                </MenuItem>
              ))}
            </MenuSection>
            <MenuItem id="replace" onAction={() => onAction("replace")} className={menuItemClass}>
              <span className="size-4" />
              {t("skins.replace")}
            </MenuItem>
            <MenuItem id="delete" onAction={() => onAction("delete")} className={dangerItemClass}>
              <span className="size-4" />
              {t("skins.delete")}
            </MenuItem>
          </Menu>
        </Popover>
      </MenuTrigger>
      {editing && (
        <TextField
          aria-label={t("skins.name")}
          value={draft}
          onChange={setDraft}
          isReadOnly={blocked}
          maxLength={64}
          autoFocus
          onBlur={() => void commitName()}
          onKeyDown={(e) => {
            if (e.key === "Enter") void commitName();
            else if (e.key === "Escape") {
              // 入力が外れるときの blur で、編集中の名前が送られないようにする
              sent.current = true;
              setEditing(false);
            }
            else e.continuePropagation();
          }}
          className="pointer-events-auto absolute inset-x-1.5 bottom-1.5"
        >
          <Input className={nameInput} onFocus={(e) => e.currentTarget.select()} />
        </TextField>
      )}
      <input
        ref={picker}
        type="file"
        accept="image/png"
        hidden
        tabIndex={-1}
        onChange={(e) => onPicked(e.currentTarget.files)}
      />
    </>
  );
}
