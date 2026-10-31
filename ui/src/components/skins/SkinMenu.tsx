import { useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconDots from "~icons/ph/dots-three";
import type { SavedSkin } from "../../bindings/SavedSkin";
import type { SkinVariant } from "../../bindings/SkinVariant";
import { useCooling, type useSkinActions } from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";
import { readSkinFile, SkinDialog, SkinFields } from "./SkinDialog";

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

const deleteButton = [
  "inline-flex h-9 items-center rounded-md px-3 text-sm text-danger",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

type Actions = Pick<ReturnType<typeof useSkinActions>, "update" | "remove" | "apply">;

/** 編集のポップアップ。開くたびに作り直し、入力は保存したスキンの今の値から始める */
function SkinEditDialog({ skin, worn, canApply, actions, isOpen, onOpenChange, onWorking }: {
  skin: SavedSkin;
  worn: boolean;
  canApply: boolean;
  actions: Actions;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onWorking: (working: boolean) => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const [name, setName] = useState(skin.name);
  const [variant, setVariant] = useState<SkinVariant>(skin.variant);
  const [file, setFile] = useState<{ name: string; png: number[] } | null>(null);
  const [saving, setSaving] = useState(false);

  const trimmed = name.trim();
  const nameChanged = trimmed !== "" && trimmed !== skin.name;
  const variantChanged = variant !== skin.variant;
  // 着ているスキンの見た目が変わる保存は Mojang へ送るので、冷却の間は保存できない。名前だけの変更は手元の記録だけなので保存できる
  const cooling = useCooling();
  const sendsToMojang = worn && canApply && (variantChanged || file !== null);
  const canSave = trimmed !== "" && (nameChanged || variantChanged || file !== null) && !(sendsToMojang && cooling);

  const pickFile = async (picked: File) => {
    try {
      setFile({ name: picked.name, png: await readSkinFile(picked) });
    } catch (e) {
      toast.showError(e);
    }
  };

  /**
   * 保存を押したときだけ送る。着ているスキンの見た目が変わるとき（モデルの切り替えとファイルの差し替え）だけ
   * 適用し直して Mojang へ反映し、名前だけの変更は手元の記録だけにする。失敗はトーストで知らせ、更新が失敗したら適用しない。
   */
  const save = async () => {
    setSaving(true);
    onWorking(true);
    try {
      await actions.update.mutateAsync({
        skinId: skin.id,
        patch: { name: nameChanged ? trimmed : null, variant: variantChanged ? variant : null, png: file?.png ?? null },
      });
      if (sendsToMojang) await actions.apply.mutateAsync({ skinId: skin.id });
      onOpenChange(false);
    } catch (e) {
      toast.showError(e);
    } finally {
      setSaving(false);
      onWorking(false);
    }
  };

  return (
    <SkinDialog
      title={t("skins.edit")}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      isPending={saving}
      canSave={canSave}
      onSave={() => void save()}
      footerStart={
        <Button
          isDisabled={saving || worn}
          onPress={() => {
            // 削除はライブラリだけで、確認は挟まない
            actions.remove.mutate({ skinId: skin.id }, { onError: (e) => toast.showError(e) });
            onOpenChange(false);
          }}
          className={deleteButton}
        >
          {t("skins.delete")}
        </Button>
      }
    >
      <SkinFields
        name={name}
        onName={setName}
        variant={variant}
        onVariant={setVariant}
        fileName={file?.name ?? null}
        onFile={(f) => void pickFile(f)}
        fileLabel={t("skins.replace")}
        disabled={saving}
      />
    </SkinDialog>
  );
}

/**
 * 保存したスキン 1 枚の「…」ボタン。押すと編集のポップアップ（名前、classic / slim、ファイルの差し替え、削除）が開く。
 * 操作の組はスキン画面と共通のものを受け取る（取り直しが終わるまでの「進行中」を 1 つにするため）。
 */
export function SkinMenu({ skin, worn, canApply, actions, disabled, onWorking }: {
  skin: SavedSkin;
  /** いま着ているスキンか */
  worn: boolean;
  canApply: boolean;
  actions: Actions;
  /** 適用・追加など、ほかの操作の最中 */
  disabled: boolean;
  /** 更新・適用の一連の最中は真。親が全てのカードとメニューを止めるのに使う */
  onWorking: (working: boolean) => void;
}) {
  const { t } = useTranslation();
  const [isOpen, setOpen] = useState(false);
  // 開くたびに入力を作り直すための番号
  const [session, setSession] = useState(0);
  const blocked = disabled || actions.update.isPending || actions.remove.isPending;

  return (
    <>
      <Button
        aria-label={t("skins.menu")}
        isDisabled={blocked}
        onPress={() => {
          setSession((n) => n + 1);
          setOpen(true);
        }}
        className={menuButton}
      >
        <IconDots aria-hidden className="size-5" />
      </Button>
      <SkinEditDialog
        key={session}
        skin={skin}
        worn={worn}
        canApply={canApply}
        actions={actions}
        isOpen={isOpen}
        onOpenChange={setOpen}
        onWorking={onWorking}
      />
    </>
  );
}
