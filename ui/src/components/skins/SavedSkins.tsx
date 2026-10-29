import { useEffect, useRef, useState } from "react";
import type { SavedSkin } from "../../bindings/SavedSkin";
import {
  useCurrentSkin,
  useImportCurrentSkin,
  useSavedSkinImage,
  useSavedSkins,
  useSkinActions,
} from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";
import { AddSkinCard } from "./AddSkinCard";
import { SkinCard } from "./SkinCard";
import { SkinMenu } from "./SkinMenu";

type CardProps = {
  skin: SavedSkin;
  selected: boolean;
  pending: boolean;
  disabled: boolean;
  onPress: () => void;
  /** 着ているスキンか（メニューの編集のあとに適用し直すかの判断に使う） */
  worn: boolean;
  canApply: boolean;
  actions: Pick<ReturnType<typeof useSkinActions>, "update" | "remove" | "apply">;
  onWorking: (working: boolean) => void;
  /** 適用・追加などの最中。メニューも止める */
  busy: boolean;
};

/** 画像の取得はカードごと（画像は変わらないのでキャッシュされる）。取れるまでは枠だけで、大きさは変わらない */
function SavedSkinCard({ skin, selected, pending, disabled, onPress, worn, canApply, actions, busy, onWorking }: CardProps) {
  const image = useSavedSkinImage(skin);
  return (
    <SkinCard
      image={image.data ?? null}
      name={skin.name}
      variant={skin.variant}
      selected={selected}
      pending={pending}
      disabled={disabled}
      onPress={onPress}
      menu={<SkinMenu skin={skin} worn={worn} canApply={canApply} actions={actions} disabled={busy} onWorking={onWorking} />}
    />
  );
}

/**
 * 保存したスキンの格子。押したスキンを適用し、いま着ているものにチェックを付ける。
 * チェックは現在のスキンの取り直しが終わるまで古いカードに残る（useSkinActions が取り直しまで進行中を保つ）。
 * canApply が偽（要ログイン・オフライン）のときは、見た目はそのままで押せないだけにする。
 */
export function SavedSkins({ accountId, canApply, blocked = false, onBusyChange }: {
  accountId: string | null;
  canApply: boolean;
  /** デフォルトのスキンの適用の最中。こちらの操作も止める */
  blocked?: boolean;
  /** 適用・マント・追加などの最中（追加の一連の合間も含む）かどうかを親へ伝える。アカウントの切り替えを止めるのに使う */
  onBusyChange?: (busy: boolean) => void;
}) {
  const saved = useSavedSkins();
  // 要ログイン・オフラインのアカウントの現在のスキンは取りにいかない（取れても 401 になるだけで、頼らない）
  const current = useCurrentSkin(canApply ? accountId : null);
  const actions = useSkinActions(accountId);
  const importSkin = useImportCurrentSkin();
  const toast = useToast();
  // 追加の一連の最中（ファイルの合間も）は、スキンの適用を始めさせない
  const [adding, setAdding] = useState(false);
  // メニューの編集（ファイルの読み込み・更新・適用の一連）の最中も同じく止める
  const [editing, setEditing] = useState(false);

  const busy = actions.busy || adding || editing || blocked;
  useEffect(() => {
    onBusyChange?.(busy);
    return () => onBusyChange?.(false);
  }, [busy, onBusyChange]);

  // いま着ているスキンの取り込みは、アカウントごとに画面を開いてから 1 回だけ。
  // 読める現在のスキン（Mojang から取れた画像）が要るので、ログインが有効なアカウントのときだけ行う
  const imported = useRef(new Set<string>());
  // 前のアカウントの表示を借りている間（isPlaceholderData）は、このアカウントのものではないので数えない
  const hasImage = !!current.data?.image && !current.isPlaceholderData;
  useEffect(() => {
    if (accountId === null || !canApply || !hasImage || imported.current.has(accountId)) return;
    imported.current.add(accountId);
    importSkin.mutate({ accountId });
  }, [accountId, canApply, hasImage, importSkin.mutate]);

  const currentHash = current.data?.sha256 ?? null;
  const applying = actions.apply.isPending ? actions.apply.variables?.skinId : undefined;

  const press = (skin: SavedSkin) => {
    // 着ているものをもう一度適用しても何も変わらない
    if (!current.isPlaceholderData && skin.sha256 === currentHash) return;
    actions.apply.mutate({ skinId: skin.id }, { onError: (e) => toast.showError(e) });
  };

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-3">
      {/* 追加はライブラリへ足すだけでログインは要らないので、canApply には縛らない */}
      <AddSkinCard disabled={actions.busy || editing || blocked} actions={actions} onAdding={setAdding} />
      {(saved.data ?? []).map((skin) => (
        <SavedSkinCard
          key={skin.id}
          skin={skin}
          selected={currentHash !== null && skin.sha256 === currentHash}
          pending={applying === skin.id}
          disabled={!canApply || busy}
          onPress={() => press(skin)}
          worn={currentHash !== null && skin.sha256 === currentHash && !current.isPlaceholderData}
          canApply={canApply}
          actions={actions}
          busy={busy}
          onWorking={setEditing}
        />
      ))}
    </div>
  );
}
