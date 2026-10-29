import { useEffect, useRef, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import { onFilesDropped } from "../../api/dragDrop";
import { IpcError } from "../../api/ipc";
import { MAX_SKIN_BYTES, type useSkinActions } from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 枠の太さと大きさは状態で変えない（ドロップ中は枠の色、無効は不透明度だけで示す）
const cardButton = [
  "absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed p-3",
  "border-line text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:border-line-strong hover:bg-surface-2 pressed:bg-surface-3",
  "data-drop-target:border-accent data-drop-target:bg-surface-2 data-drop-target:text-text",
  "disabled:opacity-50",
  ring,
].join(" ");

/** 拡張子を除いたファイル名（スキンの名前にする） */
const stem = (name: string) => name.replace(/\.[^.]*$/, "") || name;

/**
 * 「スキンを追加」のカード（保存したスキンと同じ縦横比 2:3）。押すと PNG を選べ、ウィンドウにファイルを落としても追加できる。
 * 複数のファイルは 1 つずつ順に追加し（並びを保つため）、1 つの失敗が残りを止めない。失敗はファイルごとにトーストで知らせる。
 * ログインは要らない（ライブラリへ追加するだけ）。
 */
export function AddSkinCard({ disabled, actions, onAdding }: {
  disabled: boolean;
  /** 親（SavedSkins）と同じ操作の組。適用と追加を同時に走らせないため、取り直しの管理を 1 つにする */
  actions: Pick<ReturnType<typeof useSkinActions>, "add" | "addFromPath">;
  /** 追加の一連（ファイルの合間も含む）の最中は真。親がスキンのカードを押せなくするのに使う */
  onAdding: (adding: boolean) => void;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const [dragging, setDragging] = useState(false);
  const [working, setWorking] = useState(false);
  const picker = useRef<HTMLInputElement>(null);

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

  const onPicked = (files: FileList | null) => {
    const list = Array.from(files ?? []);
    // 同じファイルをもう一度選べるように空にする
    if (picker.current) picker.current.value = "";
    if (latest.current.blocked) return;
    void runAll(
      list.map((file) => async () => {
        // 読む前に大きさを見る（誤って選んだ巨大なファイルで画面を固めない）。本体の検査と同じ失敗にする
        if (file.size > MAX_SKIN_BYTES) throw new IpcError({ code: "skin.invalid", params: {}, detail: null });
        const png = Array.from(new Uint8Array(await file.arrayBuffer()));
        await latest.current.actions.add.mutateAsync({ name: stem(file.name), png });
      }),
    );
  };

  return (
    <div className="relative aspect-[2/3] w-full">
      <Button
        isDisabled={blocked}
        data-drop-target={(dragging && !blocked) || undefined}
        onPress={() => picker.current?.click()}
        className={cardButton}
      >
        <IconPlus aria-hidden className="size-6" />
        <span className="text-sm">{t("skins.add")}</span>
      </Button>
      <input
        ref={picker}
        type="file"
        accept="image/png"
        multiple
        hidden
        tabIndex={-1}
        onChange={(e) => onPicked(e.currentTarget.files)}
      />
    </div>
  );
}
