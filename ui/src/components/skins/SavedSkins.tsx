import { useEffect, useRef } from "react";
import type { SavedSkin } from "../../bindings/SavedSkin";
import {
  useCurrentSkin,
  useImportCurrentSkin,
  useSavedSkinImage,
  useSavedSkins,
  useSkinActions,
} from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";
import { SkinCard } from "./SkinCard";

type CardProps = {
  skin: SavedSkin;
  selected: boolean;
  pending: boolean;
  disabled: boolean;
  onPress: () => void;
};

/** 画像の取得はカードごと（画像は変わらないのでキャッシュされる）。取れるまでは枠だけで、大きさは変わらない */
function SavedSkinCard({ skin, selected, pending, disabled, onPress }: CardProps) {
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
    />
  );
}

/**
 * 保存したスキンの格子。押したスキンを適用し、いま着ているものにチェックを付ける。
 * チェックは現在のスキンの取り直しが終わるまで古いカードに残る（useSkinActions が取り直しまで進行中を保つ）。
 * canApply が偽（要ログイン・オフライン）のときは、見た目はそのままで押せないだけにする。
 */
export function SavedSkins({ accountId, canApply }: { accountId: string | null; canApply: boolean }) {
  const saved = useSavedSkins();
  const current = useCurrentSkin(accountId);
  const actions = useSkinActions(accountId);
  const importSkin = useImportCurrentSkin();
  const toast = useToast();

  // いま着ているスキンの取り込みは、アカウントごとに画面を開いてから 1 回だけ。
  // 読める現在のスキン（Mojang から取れた画像）が要るので、ログインが有効なアカウントのときだけ行う
  const imported = useRef(new Set<string>());
  const hasImage = !!current.data?.image;
  useEffect(() => {
    if (accountId === null || !canApply || !hasImage || imported.current.has(accountId)) return;
    imported.current.add(accountId);
    importSkin.mutate({ accountId });
  }, [accountId, canApply, hasImage, importSkin.mutate]);

  const currentHash = current.data?.sha256 ?? null;
  const applying = actions.apply.isPending ? actions.apply.variables?.skinId : undefined;

  const press = (skin: SavedSkin) => {
    // 着ているものをもう一度適用しても何も変わらない
    if (skin.sha256 === currentHash) return;
    actions.apply.mutate({ skinId: skin.id }, { onError: (e) => toast.showError(e) });
  };

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-3">
      {/* 先頭に「スキンを追加」のカードが入る（後のタスク） */}
      {(saved.data ?? []).map((skin) => (
        <SavedSkinCard
          key={skin.id}
          skin={skin}
          selected={currentHash !== null && skin.sha256 === currentHash}
          pending={applying === skin.id}
          disabled={!canApply || actions.busy}
          onPress={() => press(skin)}
        />
      ))}
    </div>
  );
}
