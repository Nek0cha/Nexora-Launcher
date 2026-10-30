import { useEffect, useRef, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import { onFilesDropped } from "../../api/dragDrop";
import type { SkinVariant } from "../../bindings/SkinVariant";
import type { useSkinActions } from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";
import { readSkinFile, SkinDialog, SkinFields, stem } from "./SkinDialog";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 枠の太さと大きさは状態で変えない（ドロップ中は枠の色、無効は不透明度だけで示す）
const cardButton = [
  "absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed px-1 py-3",
  "border-line text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:border-line-strong hover:bg-surface-2 pressed:bg-surface-3",
  "data-drop-target:border-accent data-drop-target:bg-surface-2 data-drop-target:text-text",
  "disabled:opacity-50",
  ring,
].join(" ");

type Actions = Pick<ReturnType<typeof useSkinActions>, "add" | "addFromPath" | "update">;

/** 追加のポップアップ。ファイルを選び、名前とモデルを決めて、保存を押したときだけライブラリへ足す */
function AddSkinDialog({ actions, isOpen, onOpenChange, onAdding }: {
  actions: Actions;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onAdding: (adding: boolean) => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const [name, setName] = useState("");
  const [variant, setVariant] = useState<SkinVariant>("classic");
  const [file, setFile] = useState<{ name: string; png: number[] } | null>(null);
  // 名前を自分で触ったら、ファイルを選び直しても上書きしない
  const [edited, setEdited] = useState(false);
  const [saving, setSaving] = useState(false);

  const trimmed = name.trim();
  const canSave = file !== null && trimmed !== "";

  const pickFile = async (picked: File) => {
    try {
      const png = await readSkinFile(picked);
      setFile({ name: picked.name, png });
      if (!edited) setName(stem(picked.name));
    } catch (e) {
      toast.showError(e);
    }
  };

  const save = async () => {
    if (!file) return;
    setSaving(true);
    onAdding(true);
    try {
      const added = await actions.add.mutateAsync({ name: trimmed, png: file.png });
      // 追加ではモデルを指定できないので、選んだモデルと違えば更新で直す
      if (added.variant !== variant) await actions.update.mutateAsync({ skinId: added.id, patch: { name: null, variant, png: null } });
      onOpenChange(false);
    } catch (e) {
      toast.showError(e);
    } finally {
      setSaving(false);
      onAdding(false);
    }
  };

  return (
    <SkinDialog
      title={t("skins.add")}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      isPending={saving}
      canSave={canSave}
      onSave={() => void save()}
    >
      <SkinFields
        name={name}
        onName={(n) => {
          setEdited(true);
          setName(n);
        }}
        variant={variant}
        onVariant={setVariant}
        fileName={file?.name ?? null}
        onFile={(f) => void pickFile(f)}
        fileLabel={t("skins.chooseFile")}
        disabled={saving}
      />
    </SkinDialog>
  );
}

/**
 * 「スキンを追加」のカード（保存したスキンと同じ縦横比 2:3）。押すと追加のポップアップが開き、
 * ウィンドウに PNG を落とすと（複数も可）そのままライブラリへ追加する。
 * ドロップした複数のファイルは 1 つずつ順に追加し（並びを保つため）、1 つの失敗が残りを止めない。失敗はファイルごとにトーストで知らせる。
 * ログインは要らない（ライブラリへ追加するだけ）。
 */
export function AddSkinCard({ disabled, actions, onAdding }: {
  disabled: boolean;
  /** 親（SavedSkins）と同じ操作の組。適用と追加を同時に走らせないため、取り直しの管理を 1 つにする */
  actions: Actions;
  /** 追加の一連（ファイルの合間も含む）の最中は真。親がスキンのカードを押せなくするのに使う */
  onAdding: (adding: boolean) => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const [dragging, setDragging] = useState(false);
  const [working, setWorking] = useState(false);
  const [isOpen, setOpen] = useState(false);
  // 開くたびに入力を作り直すための番号
  const [session, setSession] = useState(0);

  // ドロップの受け口は 1 度だけ登録するので、最新の値は ref 越しに読む
  const blocked = disabled || working;
  const latest = useRef({ blocked, actions, toast, onAdding });
  latest.current = { blocked, actions, toast, onAdding };
  const running = useRef(false);

  const runAll = async (jobs: Array<() => Promise<unknown>>) => {
    if (running.current || jobs.length === 0) return;
    running.current = true;
    setWorking(true);
    latest.current.onAdding(true);
    try {
      for (const job of jobs) {
        try {
          await job();
        } catch (e) {
          latest.current.toast.showError(e);
        }
      }
    } finally {
      running.current = false;
      setWorking(false);
      latest.current.onAdding(false);
    }
  };

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onFilesDropped({
      enter: () => {
        if (!latest.current.blocked) setDragging(true);
      },
      leave: () => setDragging(false),
      drop: (paths) => {
        setDragging(false);
        if (latest.current.blocked) return;
        // 拡張子では選り分けない（可否は本体が決める）
        void runAll(paths.map((path) => () => latest.current.actions.addFromPath.mutateAsync({ path })));
      },
    })
      .then((fn) => {
        // 登録が終わる前に閉じたときは、ここで解除する
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="relative aspect-[2/3] w-full">
      <Button
        isDisabled={blocked}
        data-drop-target={(dragging && !blocked) || undefined}
        onPress={() => {
          setSession((n) => n + 1);
          setOpen(true);
        }}
        className={cardButton}
      >
        <IconPlus aria-hidden className="size-6" />
        <span className="whitespace-nowrap text-sm">{t("skins.add")}</span>
      </Button>
      <AddSkinDialog key={session} actions={actions} isOpen={isOpen} onOpenChange={setOpen} onAdding={onAdding} />
    </div>
  );
}
