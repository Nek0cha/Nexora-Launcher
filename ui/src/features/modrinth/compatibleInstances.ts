import type { Instance } from "../../bindings/Instance";
import type { SearchHit } from "../../bindings/SearchHit";

/** 検索結果の MOD が入るインスタンス。Minecraft のバージョンが合い、ローダーが一致するもの。 */
export function compatibleInstances(hit: SearchHit, instances: Instance[]): Instance[] {
  return instances.filter((instance) => {
    if (instance.loader === "vanilla") return false;
    if (!hit.versions.includes(instance.mcVersion)) return false;
    // Quilt は Fabric の MOD も読み込める
    const accepted = instance.loader === "quilt" ? ["quilt", "fabric"] : [instance.loader];
    return hit.categories.some((category) => accepted.includes(category));
  });
}
