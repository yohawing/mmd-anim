# Motion Golden Oracle Quality Report

This report is generated from a compact quality snapshot. It identifies each model/motion pair and its outcome, but does not include absolute asset paths, per-frame data, or per-bone dumps.

## Result at a glance

Of 128 selected asset pairs, 21 reached numeric comparison and 1 passed the current parity thresholds.

The counts form an execution funnel. A pair that stops at an earlier stage is still part of the campaign and is classified below.

## Run and provenance

| Field | Value |
| --- | --- |
| Commit SHA | c946304d1c3760ec7951a05a06d055485e30ee0d |
| Repository state | clean |
| MMD version (self-reported) | 9.32-x64 |
| MMDDumper version (self-reported) | MMDDumper-e51a439 |
| MMD version source | config-self-reported |
| MMDDumper version source | config-self-reported |
| MMD executable SHA-256 | 07516fd3bf1e6b1339836b6773a156f61bdd6f848eeb621fdda012375df313a1 |
| Timestamp | 2026-08-25T09:06:45+09:00 |
| Sampling policy | frozen-library-128x5-v1 |
| Manifest SHA-256 | 281616cb18a24ff6194c7ab3e514b9581e9c4a8c59bbcc31da484970c366727c |

## Execution funnel

| Stage | Cases |
| --- | ---: |
| Discovered | 128 |
| Selected | 128 |
| Prepared | 56 |
| Recorded | 31 |
| Compared | 21 |
| Passed | 1 |

### How to read the funnel

| Stage | Meaning |
| --- | --- |
| Prepared | A PMM scene was built for the pair. |
| Recorded | MMD played the scene and produced oracle samples. |
| Compared | The oracle samples and `mmd-anim` output had comparable fields. |
| Passed | Every reported metric stayed within the current thresholds. |

## Parity thresholds

| Metric | Threshold |
| --- | ---: |
| translationMaxError | 0.003 |
| translationRmsError | 0.001 |
| rotationMaxAngleRad | 0.003 |
| rotationRmsAngleRad | 0.001 |
| maxAbsError | 0.003 |

## Metric distributions

| Metric | p50 | p95 | p99 | max |
| --- | ---: | ---: | ---: | ---: |
| translationMaxError | 2.53557300568 | 12.3038606644 | 12.5500001907 | 12.5500001907 |
| translationRmsError | 0.46462298869 | 2.90261227065 | 4.00963327758 | 4.00963327758 |
| rotationMaxAngleRad | 0.605169653893 | 1.7592805624 | 2.92146682739 | 2.92146682739 |
| rotationRmsAngleRad | 0.239958300256 | 0.658205959268 | 2.29738123317 | 2.29738123317 |
| maxAbsError | 2.53557300568 | 12.3038606644 | 12.5500001907 | 12.5500001907 |

## Failure classifications

| Classification | Cases | Meaning |
| --- | ---: | --- |
| compare-fields | 10 | The recording contained no fields that could be compared. |
| prepare | 72 | The PMM scene could not be prepared. |
| record | 25 | MMD did not produce a usable recording. |
| threshold | 20 | At least one numeric metric exceeded its threshold. |

## Feature summaries

| Tag | Selected | Compared | Passed |
| --- | ---: | ---: | ---: |
| bone-motion | 128 | 21 | 1 |

## Category summaries

| Tag | Selected | Compared | Passed |
| --- | ---: | ---: | ---: |
| deterministic-library-sample | 128 | 21 | 1 |

## Worst cases

| Case | Model | Motion | Metric | Value | Result |
| --- | --- | --- | --- | ---: | --- |
| library-0085-93262b8f9d40 | 桃川うさぴ_v1.02_規約改定/桃川うさぴ_v1.02.pmx | 096_スイマジ/スイマジ/sweetmagic-left.vmd | maxAbsError | 12.5500001907 | fail |
| library-0085-93262b8f9d40 | 桃川うさぴ_v1.02_規約改定/桃川うさぴ_v1.02.pmx | 096_スイマジ/スイマジ/sweetmagic-left.vmd | translationMaxError | 12.5500001907 | fail |
| library-0016-ec39e66d60d2 | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_Normal/Kizuna ai_Normal（no toon）.pmx | 《反转童话》带感MMD舞蹈动作数据片段2_by_兰音Reine_efa6e667355ec2e81f3f78a2c5292bd4/《反转童话》带感MMD舞蹈动作数据片段2.0/右伴舞.vmd | maxAbsError | 12.3038606644 | fail |
| library-0016-ec39e66d60d2 | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_Normal/Kizuna ai_Normal（no toon）.pmx | 《反转童话》带感MMD舞蹈动作数据片段2_by_兰音Reine_efa6e667355ec2e81f3f78a2c5292bd4/《反转童话》带感MMD舞蹈动作数据片段2.0/右伴舞.vmd | translationMaxError | 12.3038606644 | fail |
| library-0049-edef068672d0 | xzy_MMD/zakharova/zakharova_MMD（武器）.pmx | Best Friends/motion.vmd | maxAbsError | 12.0810251236 | fail |
| library-0049-edef068672d0 | xzy_MMD/zakharova/zakharova_MMD（武器）.pmx | Best Friends/motion.vmd | translationMaxError | 12.0810251236 | fail |
| library-0016-ec39e66d60d2 | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_Normal/Kizuna ai_Normal（no toon）.pmx | 《反转童话》带感MMD舞蹈动作数据片段2_by_兰音Reine_efa6e667355ec2e81f3f78a2c5292bd4/《反转童话》带感MMD舞蹈动作数据片段2.0/右伴舞.vmd | translationRmsError | 4.00963327758 | fail |
| library-0104-c6b4e7f5ac37 | Phaetusa (Default)/Weapon Sword.pmx | p_ブルーハワイレモン/mualani_genshin/facial.vmd | maxAbsError | 12 | fail |
| library-0104-c6b4e7f5ac37 | Phaetusa (Default)/Weapon Sword.pmx | p_ブルーハワイレモン/mualani_genshin/facial.vmd | translationMaxError | 12 | fail |
| library-0095-ec5f5988903f | xzy_MMD/roland_02/roland_02_MMD（武器）.pmx | BlackPink - So Hot/haku.vmd | maxAbsError | 10.6228475571 | fail |
| library-0095-ec5f5988903f | xzy_MMD/roland_02/roland_02_MMD（武器）.pmx | BlackPink - So Hot/haku.vmd | translationMaxError | 10.6228475571 | fail |
| library-0085-93262b8f9d40 | 桃川うさぴ_v1.02_規約改定/桃川うさぴ_v1.02.pmx | 096_スイマジ/スイマジ/sweetmagic-left.vmd | translationRmsError | 2.90261227065 | fail |
| library-0104-c6b4e7f5ac37 | Phaetusa (Default)/Weapon Sword.pmx | p_ブルーハワイレモン/mualani_genshin/facial.vmd | translationRmsError | 2.82842712475 | fail |
| library-0095-ec5f5988903f | xzy_MMD/roland_02/roland_02_MMD（武器）.pmx | BlackPink - So Hot/haku.vmd | translationRmsError | 2.80790049566 | fail |
| library-0059-90466e9f8648 | kanon/Kanon.pmx | 117_虎視眈々モーションセット/虎視眈々モーションセット/虎視眈々_黒子.vmd | maxAbsError | 7.78625917435 | fail |
| library-0059-90466e9f8648 | kanon/Kanon.pmx | 117_虎視眈々モーションセット/虎視眈々モーションセット/虎視眈々_黒子.vmd | translationMaxError | 7.78625917435 | fail |
| library-0049-edef068672d0 | xzy_MMD/zakharova/zakharova_MMD（武器）.pmx | Best Friends/motion.vmd | translationRmsError | 2.49887418775 | fail |
| library-0078-318cec9536e2 | 035_Sour式巡音ルカVer.1.01/Black.pmx | DECO27 x PinocchioP - (Not) A Devil feat. Hatsune Miku/(Not) A Devil - Motion (Nikisa San).vmd | rotationRmsAngleRad | 2.29738123317 | fail |
| library-0059-90466e9f8648 | kanon/Kanon.pmx | 117_虎視眈々モーションセット/虎視眈々モーションセット/虎視眈々_黒子.vmd | translationRmsError | 2.08055186763 | fail |
| library-0108-9fd80b2a545d | aria/aria.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/シンプルウォーク.vmd | maxAbsError | 3.49297142029 | fail |

## Asset pair results

Asset labels are relative to the local PMX and VMD library roots; machine-specific absolute paths are omitted.

<details><summary>Show all 128 asset pairs</summary>

| Case | Model | Motion | Outcome | Classifications |
| --- | --- | --- | --- | --- |
| library-0001-67c1e1d81d6e | 星降あめる_Ver1.22_規約改定/孫の手.pmx | 118_Elect Motion/Elect Motion/Reimu.vmd | Preparation failed | prepare |
| library-0002-79c8930b9281 | xzy_MMD/yammyn/yammyn_new_MMD(角色).pmx | 「ロミシン」DIVAっぽいどverハツネミク用/「ロミシン」DIVAっぽいどverハツネミク用/Motion/ロミシン（DIVAハツネミク）モーション.vmd | Recording failed | record |
| library-0003-5475cd50872a | xzy_MMD/griffin_02/griffin_02_MMD.pmx | 09_メランコリ・ナイト/メランコリ・ナイト.vmd | Preparation failed | prepare |
| library-0004-7cec317fb13c | Lewis (Default)/GirlsFrontline LewisDefault.pmx | p_ブルーハワイレモン/mualani_genshin/dance.vmd | Preparation failed | prepare |
| library-0005-85a194faad5b | xzy_MMD/shadow/shadow_MMD(ALL).pmx | ワールドイズマイン_motion/ワールドイズマイン（男性モデル向け）.vmd | Preparation failed | prepare |
| library-0006-3030f61e36b1 | 依露希尔—尤菲莉亚/武器.pmx | 123_響喜乱舞/Face&mouth by Ueki·YGi &银杏卷.vmd | Recording failed | record |
| library-0007-a381c4fbd37a | 布伦妮_by_原神/书.pmx | Happy_Halloweenセット/1人目フルモーション.vmd | Preparation failed | prepare |
| library-0008-af1583d333c5 | xzy_MMD/yammyn/yammyn_new_MMD.pmx | きゃぴ モーション/VMD/kyapi_marin_C.vmd | Preparation failed | prepare |
| library-0009-989b3bde7ed0 | on_YUKIKAZE_v095/on_YUKIKAZE_v095/onda_mod_YUKIKAZE_v095.pmx | アニメ走りっぽいモーション/アニメ走りっぽいモーション/Rick式走りモーション08（足以外微動だにせず）.vmd | Preparation failed | prepare |
| library-0010-68a59b631c48 | xzy_MMD/shadow/shadow_MMD(武器).pmx | Winter Bouquet モーション/モーション配布/winterluv.vmd | Not comparable | compare-fields |
| library-0011-957a915b36c1 | Voymastina (Erwin)/GirlsFrontline VoymastinaFormalSuit.pmx | あっかんべぃべ モーション/あっかんべいべ_モーション配布.vmd | Preparation failed | prepare |
| library-0012-3b1192f2137c | Tda式初音ミクV4X_Ver1.00/model.pmx | IA HIGHER モーション/ボールモーション/ball_bule.vmd | Preparation failed | prepare |
| library-0013-2f441da59c12 | NIKKE Rupee/Rupee.pmx | BlackPink - So Hot/Motion.vmd | Recording failed | record |
| library-0014-6f943513d2ee | ROSE/PMX/ROSE_high.pmx | アニメ走りっぽいモーション/アニメ走りっぽいモーション/Rick式走りモーション06.vmd | Preparation failed | prepare |
| library-0015-2aeaa05e2ae2 | NIKKE Elegg/Elegg.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/廃人向け/6666モーション素材/歩き/レイヤ0-4 歩きおじぎサンプル.vmd | Recording failed | record |
| library-0016-ec39e66d60d2 | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_Normal/Kizuna ai_Normal（no toon）.pmx | 《反转童话》带感MMD舞蹈动作数据片段2_by_兰音Reine_efa6e667355ec2e81f3f78a2c5292bd4/《反转童话》带感MMD舞蹈动作数据片段2.0/右伴舞.vmd | Over threshold | threshold |
| library-0017-e2c58bf7b6f8 | Phaetusa (Dorm)/Weapon Sword.pmx | 美少女戦士まつりみっとZero モーション/美少女戦士まつりみっとZero.vmd | Preparation failed | prepare |
| library-0018-409b127aabb8 | Suomi (SparklingOcean)/GirlsFrontline SuomiSwimwear.pmx | wavefile_v2.vmd | Recording failed | record |
| library-0019-364babcbfe75 | EL-Pr242 KANANAGI _ 奏渚 汐藍/プロップ-脱ぎビキニ上(雑).pmx | viis-Jeb Nid Nid/act柊さんた_viis-Jeb Nid Nid.vmd | Preparation failed | prepare |
| library-0020-afbad8b1a820 | ■Tac式ミク配布用/Tac式ミクVer1.3.0【高速移動対応版】/ver1.3.0_スク水/Tac式ミクver1.3.0（ワンピ水着）高速移動対応版.pmx | MMD Howl Challenge Ver1.00/MMD_Howl_Challenge/MMD_Howl_Challenge/Model-specific motions(モデル別モーション)/MMD_HOWL_ShortMotion(Voisona雨衣).vmd | Preparation failed | prepare |
| library-0021-ae42a64dde38 | つみ式ミクさんv4/つみ式ミクさんv4/つみ式ミクさんv4.pmx | レモンメロンクッキー/Sour式レモンメロンクッキー.vmd | Over threshold | threshold |
| library-0022-acd5d66fae22 | NIKKE Dorothy (alternate)/dorothy_mmd.pmx | 私、アイドル宣言モーション/私、アイドル宣言モーション/私、アイドル宣言 モーション.vmd | Preparation failed | prepare |
| library-0023-5ac6e50275cb | xzy_MMD/qiuyu/qiuyu_MMD(角色).pmx | 星街すいせい×宝鐘マリン モーション/vmd/星街すいせい.vmd | Preparation failed | prepare |
| library-0024-234e152ddf06 | 希希芙/希希芙.pmx | Overdose(ゆり様モーション)_カメラ表情リップ/原曲音源合わせ/Overdose_eye.vmd | Recording failed | record |
| library-0025-029c96a87fba | 千夏 午后茶歇/武器.pmx | アニメ走りっぽいモーション/アニメ走りっぽいモーション/Rick式走りモーション04.vmd | Preparation failed | prepare |
| library-0026-47a850d6fa03 | Cheyanne (Maiden Debut)/Bag.pmx | Happy_Halloweenセット/2人目フルモーション.vmd | Preparation failed | prepare |
| library-0027-f1e54bae1f4d | Balthilde (Dorm)/GirlsFrontline BalthildeRestroom.pmx | 124_[A]ddiction_モーション/[A]ddiction_モーション/[A]ddiction_男性用.vmd | Preparation failed | prepare |
| library-0028-cf131060af2e | 芙洛伦原皮/GirlsFrontline FlorenceDefault.pmx | モンローウォーク修正版/モンローウォーク修正版/Tda式用モンローウォーク(ハイヒール).vmd | Recording failed | record |
| library-0029-c81f05bcdb93 | NIKKE Modernia/modernia_mmd.pmx | 122_桃源恋歌モーション配布用（改善）/桃源恋歌モーション配布用/TOGENRENKA(foot-IK's Turn made to 0 足IK回転０化).vmd | Preparation failed | prepare |
| library-0030-0c011fd475a1 | xzy_MMD/qingni/qingni_MMD(武器).pmx | p_にぎにぎにじたうん/facial_mizuki.vmd | Not comparable | compare-fields |
| library-0031-ece0a3a4bff9 | xzy_MMD/kitty02/kitty02_MMD(ALL).pmx | 《反转童话》带感MMD舞蹈动作数据片段2_by_兰音Reine_efa6e667355ec2e81f3f78a2c5292bd4/《反转童话》带感MMD舞蹈动作数据片段2.0/左伴舞.vmd | Recording failed | record |
| library-0032-4dc41c64ff70 | Helen (Dorm)/GirlsFrontline HelenRestroom.pmx | GFRIEND - Fingertip/motion.vmd | Over threshold | threshold |
| library-0033-471efc655fec | ぽんぷ長式Jervis（ジャーヴィス）/ぽんぷ長式Jervis（ジャーヴィス）/ぽんぷ長式Jervis（ジャーヴィス）_v1.pmx | Lap Tap Love モーション/Lap Tap Love モーション/Lap Tap Love (あにまさ式ミクver)モーション.vmd | Preparation failed | prepare |
| library-0034-f13e0ba0eb22 | 【绮良良】_by_原神_3ae87334e869b584e1e665d027c956ee/变身形态.pmx | Happy_Halloweenセット/リップ表情のみ/２人目表情リップ目線のみ.vmd | Recording failed | record |
| library-0035-3daba9aebe11 | 【异环】小吱/小吱/小吱.pmx | 134_ベノム_配布用/ベノム_配布用/モーション_by永遠の1/ベノム_モーション.vmd | Preparation failed | prepare |
| library-0036-0feaa9bce2ed | xzy_MMD/roland/roland_MMD.pmx | 桃源恋歌配布用motion/ノーマルTda式用.vmd | Preparation failed | prepare |
| library-0037-7418dbf7f3e3 | 蕾米埃尔 3.1/蕾米埃尔-白/蕾米埃尔-白/武器翅膀2.pmx | 130_劣等上等2人用モーションセット/劣等上等2人用モーションセット/劣等上等_右_三日月宗近D配布用.vmd | Recording failed | record |
| library-0038-d071a77c1c3a | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_Normal/Kizuna ai_Normal.pmx | 095_melancholic_motion/melancholic_motion/melancholic_all.vmd | Over threshold | threshold |
| library-0039-0990f8a4ed17 | 归龙潮-光-建模优化版/光_无背包.pmx | Deja Vu motion + face/Deja_Vu motion + face.vmd | Recording failed | record |
| library-0040-77361013601c | xzy_MMD/katerina/katerina/katerina_MMD(武器）.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/旋回サンプル.vmd | Not comparable | compare-fields |
| library-0041-350507066143 | 神里绫华-模之屋_by_原神_57919b356c6bdd2aefc1f1e8575521f8/神里绫华.pmx | 107_メグメグFダンスモーションデータ/メグメグファイアー☆エンドレスナイトダンスモーションデータ配布用(タイトル長げぇｗ/メグメグ☆ アイマス用.vmd | Recording failed | record |
| library-0042-87faf6e53fb1 | 希希芙/葡萄.pmx | 094_ハッピーシンセサイザモーションセット/ハッピーシンセサイザモーションセット/ハッピーシンセサイザモーション.vmd | Not comparable | compare-fields |
| library-0043-81604f9d37a2 | 【芙宁娜】_by_原神_dd7a8a03e7a7dfa6593053d639fa3025/章鱼.pmx | 141_スーサイドパレエド_配布用/スーサイドパレエド 配布用/さとく式鶯丸.vmd | Preparation failed | prepare |
| library-0044-31a7417c3ea6 | AIVoice2音街ウナ（公式モデル）/AIVoice2音街ウナ（公式モデル).pmx | Lap Tap Love モーション/Lap Tap Love モーション/Lap Tap Love (コロン式ミクver)モーション.vmd | Preparation failed | prepare |
| library-0045-a7e2cf8ce173 | 丝柯克_by_原神_8920e33db8f4548302e3c0cbb7192bcd/丝柯克_黑.pmx | 103_サイバーサンダーサイダーモーション/サイバーサンダーサイダーモーション/サイバーサンダーサイダー.vmd | Recording failed | record |
| library-0046-0da5c494a201 | 索米原皮/GirlsFrontline SuomiDefault.pmx | iMarine_MarineMirage_dance/iMarine_MarineMirage_dance/MarineMirrage_dance.vmd | Preparation failed | prepare |
| library-0047-a0fcd0776c7c | 叶瞬光/武器.pmx | アニメ走りっぽいモーション/アニメ走りっぽいモーション/Rick式走りモーション03.vmd | Preparation failed | prepare |
| library-0048-6fda4d786d7f | 希希芙/武器.pmx | 130_劣等上等2人用モーションセット/劣等上等2人用モーションセット/劣等上等_左_小狐丸D配布用.vmd | Not comparable | compare-fields |
| library-0049-edef068672d0 | xzy_MMD/zakharova/zakharova_MMD（武器）.pmx | Best Friends/motion.vmd | Over threshold | threshold |
| library-0050-ed9c16e85d49 | xzy_MMD/rhine/rhine_MMD（ALL）.pmx | PRISTIN V - Get It/motion.vmd | Recording failed | record |
| library-0051-cd39faa71f8d | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/傘.pmx | サキミダレダンス（Sakimidare Apparade）モーション/SakimidareApparade_upperbody.vmd | Not comparable | compare-fields |
| library-0052-9cad6d86f0f0 | xzy_MMD/roland/roland_MMD（武器）.pmx | FREELY_TOMORROW_motion/FREELY_TOMORROW_motion/FREELY_TOMORROW.vmd | Not comparable | compare-fields |
| library-0053-4a5d671f1e3b | xzy_MMD/sharp/sharp_MMD（角色）.pmx | モンローウォーク修正版/モンローウォーク修正版/ままま式GUMI用モンローウォーク.vmd | Recording failed | record |
| library-0054-b51b119e3a05 | どっと式初音ミク_V3_ver.2.02/どっと式初音ミク_V3_ver.2.02/どっと式初音ミク_V3体.pmx | 1925/1925/【トレス元】モーション/1925左.vmd | Preparation failed | prepare |
| library-0055-8b961f236e8c | 【模之屋】砂糖_by_原神_b7478ddcd12885599962e204eb493614/砂糖.pmx | p_チェリーポップ/facial_mococo.vmd | Recording failed | record |
| library-0056-5fc03dfcfae4 | 初音ミクver.2.1/初音ミクver.2.1/初音ミクver.2.1(準標準ボーン追加).pmx | 124_[A]ddiction_モーション/[A]ddiction_モーション/[A]ddiction_Tda式.vmd | Preparation failed | prepare |
| library-0057-e142e86eea5e | C-MMD/C-MMD/C(toonoff).pmx | レモンメロンクッキー/Rem式レモンメロンクッキー.vmd | Over threshold | threshold |
| library-0058-b94d6eb22ea1 | 【异环】安魂曲_by_异环/安魂曲.pmx | RainOnMe_Motion/RainOnMe_motion.vmd | Preparation failed | prepare |
| library-0059-90466e9f8648 | kanon/Kanon.pmx | 117_虎視眈々モーションセット/虎視眈々モーションセット/虎視眈々_黒子.vmd | Over threshold | threshold |
| library-0060-52e89d87ef29 | xzy_MMD/breaker/breaker_unassembled_MMD(武器).pmx | アニメ走りっぽいモーション/アニメ走りっぽいモーション/Rick式走りモーション09（足以外微動だにせず）.vmd | Preparation failed | prepare |
| library-0061-00a17dddc514 | EL-Pr251 Rosastout _ ローザスタウト/ローザスタウト.pmx | おねがいダーリン モーション v1.2/おねがいダーリン_Tda式.vmd | Preparation failed | prepare |
| library-0062-72e3bc3729f5 | xzy_MMD/griffin_02/griffin_02_MMD(角色).pmx | 愛包ダンスホール_Motion/愛包ダンスホール_DanceMotion_HimeTanaka(MMD)_v1.1.vmd | Preparation failed | prepare |
| library-0063-054f15cebb69 | 【异环】法帝娅1.1/法帝娅1.1/法帝娅1.1.pmx | ねっちゅーシンドローム モーション/NecchuSyndrome_Motion_MMD.vmd | Recording failed | record |
| library-0064-7bf04265631c | 兰音Reine_国风mmd_by_兰音Reine_930489d1337aff033e72affba1310c05/兰音Reine_国风mmd_ver1.0.pmx | p_えぶりでいホスト/facial_fuwawa.vmd | Recording failed | record |
| library-0065-6734ac4983b9 | xzy_MMD/breaker/breaker_unassembled_MMD(ALL).pmx | MMD KonkoKonKon Ver1.03/MMD_KonkoKonKon/MMD_KonkoKonKon/VRMLiveViewer_Data/MMD_KonKoKonkonDanceMotion-cnv-ret(AvatarSample_B用).vmd | Preparation failed | prepare |
| library-0066-572f81cb3bf0 | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/Ver.アニメ/傘.pmx | 71- Patchwork Staccato/Airi.vmd | Preparation failed | prepare |
| library-0067-5413bb283c93 | xzy_MMD/betaii/betaii_MMD(引擎+武器).pmx | p_薄ら氷心中/facial_chiori.vmd | Recording failed | record |
| library-0068-148850db96a9 | どっと式初音ミク_ハニーウィップ_ver.2.01/どっと式初音ミク_ハニーウィップ_ver.2.01/どっと式初音ミク_ハニーウィップ.pmx | Dear_cocoa_girlsモーション/Dear_cocoa_girlsモーション/Dearcocoagirls(chibimiku).vmd | Over threshold | threshold |
| library-0069-6a79b1d2a94a | xzy_MMD/sharp/sharp_MMD（引擎+武器）.pmx | Chikayori Pose/ChikayoriPose.vmd | Preparation failed | prepare |
| library-0070-bb712d6880b6 | 音街ウナ（公式モデル）SynthesizerV2/SynthesizerV2_音街ウナ(公式モデル).pmx | 恋愛フィロソフィアmotion/恋愛フィロソフィア（原曲PV音源合わせ）.vmd | Over threshold | threshold |
| library-0071-8c9ff3998d5a | 普罗米娅/普罗米娅_斗篷.pmx | 兰音Reine-白龙-舞蹈数据_by_兰音Reine_66be61fd9e437359b5ad2a6a3691bcee/白龙-motion数据-配布用-by：兰音Reine.vmd | Preparation failed | prepare |
| library-0072-af45b93dab7e | EL-Pr242 KANANAGI _ 奏渚 汐藍/プロップ-脱ぎワンピ.pmx | p_薄ら氷心中/motion.vmd | Preparation failed | prepare |
| library-0073-c75f6fbb375b | 泠鸢-公式服3.0-栗子发_by_临时映画_b5c1ce2306efb9ee9ecfbecf3002c28c/泠鸢公式服-栗子发154cm-Tpose.pmx | 122_桃源恋歌モーション配布用（改善）/桃源恋歌モーション配布用/TOGENRENKA(for Lat-type Lat式用）.vmd | Preparation failed | prepare |
| library-0074-c20a69aa051a | 蕾米埃尔 3.1/蕾米埃尔-泳装/蕾米埃尔-泳装/蕾米埃尔-泳装-带大翅膀.pmx | 110_weekender_girl/weekender_girl/wg_motionつみ式.vmd | Preparation failed | prepare |
| library-0075-8d73c95b4e97 | xzy_MMD/angelis/angelis_MMD(武器）.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/廃人向け/6666モーション素材/歩き/レイヤ0-2 シンプルウォーク.vmd | Not comparable | compare-fields |
| library-0076-16c331062157 | 鏡音リン冬(120502)/鏡音リン冬(120502)/鏡音リンA(773).pmx | 131_ヒバナ/ヒバナ/ヒバナ配布用.vmd | Preparation failed | prepare |
| library-0077-8098deb76130 | xzy_MMD/skysaber/skysaber_new_MMD.pmx | 窃窃motion_by_兰音Reine_d0232730e1f5338f3af39336d95fa1b4/qieqie_motion.vmd | Preparation failed | prepare |
| library-0078-318cec9536e2 | 035_Sour式巡音ルカVer.1.01/Black.pmx | DECO27 x PinocchioP - (Not) A Devil feat. Hatsune Miku/(Not) A Devil - Motion (Nikisa San).vmd | Over threshold | threshold |
| library-0079-b0238f2135fb | Arona_demo/Arona_demo.pmx | トンツカタンタン/act柊さんた_トンツカタンタン.vmd | Preparation failed | prepare |
| library-0080-2107e1fd3876 | Faye (FlurryCrimson)/GirlsFrontline FayeBoxing.pmx | 歩きと走り/走る10.vmd | Recording failed | record |
| library-0081-8debce7a8a89 | Kizuna Ai Model pack by kanbara-naiki/KizunaAi_birthday/Kizuna ai_Birthday.pmx | 122_桃源恋歌モーション配布用（改善）/桃源恋歌モーション配布用/TOGENRENKA(for Tda-type Tda式用）.vmd | Preparation failed | prepare |
| library-0082-f6253741ef0c | xzy_MMD/nora/nora_MMD.pmx | NIKKE Be My Star Motion/Be My Star VMD.vmd | Preparation failed | prepare |
| library-0083-b473d2a6d2fa | xzy_MMD/phoebe/phoebe_MMD(ALL).pmx | IA Conqueror モーション/IA_Conqueror_light_version.vmd | Preparation failed | prepare |
| library-0084-bcdaa3d6b0a6 | 【模之屋】申鹤_by_原神_add938b763d11efe56328b5364e5c826/申鹤.pmx | 71- Patchwork Staccato/Shizuku.vmd | Preparation failed | prepare |
| library-0085-93262b8f9d40 | 桃川うさぴ_v1.02_規約改定/桃川うさぴ_v1.02.pmx | 096_スイマジ/スイマジ/sweetmagic-left.vmd | Over threshold | threshold |
| library-0086-2b109e6311e4 | xzy_MMD/hikari/hikari_MMD(ALL).pmx | 71- Patchwork Staccato/Miku.vmd | Preparation failed | prepare |
| library-0087-138ba36929f8 | 布伦妮_by_原神/布伦妮.pmx | 愛包ダンスホール Motion/AipaiDanceHall_Motion(MMD)_Full_HimeTanaka_v1.1[f-19].vmd | Preparation failed | prepare |
| library-0088-3d57d3cb6a5e | xzy_MMD/emika/emika_MMD_pistol.pmx | MMD Howl Challenge Ver1.00/MMD_Howl_Challenge/MMD_Howl_Challenge/Model-specific motions(モデル別モーション)/MMD_HOWL_ShortMotion(Genshin基尼奇).vmd | Preparation failed | prepare |
| library-0089-20d0d32abceb | 【绮良良】_by_原神_3ae87334e869b584e1e665d027c956ee/绮良良.pmx | 愛言葉Ⅳ_モーション_ver02（Full ver.）/愛言葉Ⅳ_リップのみ（原曲合わせ）.vmd | Recording failed | record |
| library-0090-7b593ee90a99 | xzy_MMD/tatiana/tatiana_MMD(ALL).pmx | 098_galaxias_motion/galaxias_motion/galaxias!_lat.vmd | Preparation failed | prepare |
| library-0091-6eacdd6c0884 | xzy_MMD/haruka01/haruka01_MMD(ALL）.pmx | HP_motion/HP_motion_2_mod.vmd | Recording failed | record |
| library-0092-c9309b6f7329 | C-MMD/C.pmx | ワールドイズマイン_motion/ワールドイズマイン.vmd | Preparation failed | prepare |
| library-0093-2c177022c9e6 | Lainie (Operation Butterfly)/GirlsFrontline LeneSSR0101.pmx | 兰音Reine-云月谣-舞蹈数据_by_doley_f3042a76828f7fdeab6b1b7565771bc2/云月谣-motion数据-配布用-by：兰音Reine.vmd | Preparation failed | prepare |
| library-0094-7dd99d921f44 | Florence (Marvelous Yam Pastry)/GirlsFrontline FlorencePajama.pmx | MMD KonkoKonKon Ver1.03/MMD_KonkoKonKon/MMD_KonkoKonKon/Background_Sample/For_MMD/MMD_KonKoKonkonDanceMotion(TaroMikuKai).vmd | Preparation failed | prepare |
| library-0095-ec5f5988903f | xzy_MMD/roland_02/roland_02_MMD（武器）.pmx | BlackPink - So Hot/haku.vmd | Over threshold | threshold |
| library-0096-7e9943782e28 | レーシングミク2012_アノマロ5th/レーシングミク2012_アノマロ5th/レーシングミク2012_アノマロ5th.pmx | お姉さま♡ラヴコール モーション/La+お姉さま♡ラヴコール.vmd | Over threshold | threshold |
| library-0097-917e15cbd690 | 夏洛蒂皮肤（夏日）/夏洛蒂_夏日.pmx | MMD KonkoKonKon Ver1.03/MMD_KonkoKonKon/MMD_KonkoKonKon/MMD_KonKoKonkonDanceMotion.vmd | Preparation failed | prepare |
| library-0098-0b442ec82cf9 | xzy_MMD/iva/iva_MMD（角色）.pmx | CHANGE/CH4NGE_Light.vmd | Preparation failed | prepare |
| library-0099-d3bca754695f | xzy_MMD/cherub/cherub_MMD(角色).pmx | ichikazero_soukaidakkaisunlight_VMD/爽快奪回Sunlight_ichika.vmd | Preparation failed | prepare |
| library-0100-ff64832c8925 | xzy_MMD/ether/ether_MMD.pmx | ラビットホール/ラビットホール.vmd | Preparation failed | prepare |
| library-0101-58e79f2ac1e3 | どっと式初音ミク_V3_ver.2.02/どっと式初音ミク_V3_ver.2.02/編集用.pmx | 117_虎視眈々モーションセット/虎視眈々モーションセット/虎視眈々_緑間.vmd | Over threshold | threshold |
| library-0102-667eda9206e2 | YYB式初音ミクver1.02/YYB式初音ミクv1.02.pmx | IA Conqueror モーション/IA_Conqueror_full_key_version.vmd | Preparation failed | prepare |
| library-0103-86d616a7d9d8 | Konpaku Youmu (Short Skirt)/Konpaku Youmu (Short Skirt)/Konpaku Youmu 2.00 (little skirt).pmx | おねがいダーリン モーション v1.2/おねがいダーリン_プロ生ちゃん.vmd | Preparation failed | prepare |
| library-0104-c6b4e7f5ac37 | Phaetusa (Default)/Weapon Sword.pmx | p_ブルーハワイレモン/mualani_genshin/facial.vmd | Over threshold | threshold |
| library-0105-1a77dddadecf | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/傘_大.pmx | 刹那プラス配布用/刹那プラス配布用/刹那プラス.vmd | Over threshold | threshold |
| library-0106-01bbd84adf61 | 莉奈娅_by_原神/莉奈娅.pmx | 兰音Reine-菟园-舞蹈数据_by_兰音Reine_af39657aa37f388f8f7b55c8066dd546/菟园-motion数据-配布用-by：兰音Reine.vmd | Recording failed | record |
| library-0107-f19af2d22aeb | xzy_MMD/yammyn/yammyn_new_MMD(引擎+武器).pmx | ラブミ！ モーション/Shinkyoku_scene1.vmd | Preparation failed | prepare |
| library-0108-9fd80b2a545d | aria/aria.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/シンプルウォーク.vmd | Over threshold | threshold |
| library-0109-202817716729 | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/Ver.アニメ/ぽんぷ長式大和_艤装＿アニメ.pmx | p_ほんまやで_なんでやねん_しらんけど/motion_2.vmd | Preparation failed | prepare |
| library-0110-3b1e332a1784 | EL-Pr252 KOTORA _ 虎寅/小寅 百合ヰ(体操服).pmx | MMD Howl Challenge Ver1.00/MMD_Howl_Challenge/MMD_Howl_Challenge/MMD_HOWL_ShortMotion.vmd | Preparation failed | prepare |
| library-0111-ce1af0c0e844 | nijikawa_laki_ver1.0/nijikawa_laki_ver1.0.pmx | 134_ベノム_配布用/ベノム_配布用/リップ表情_カメラ_byノン/ベノム_リップ表情目線のみ.vmd | Pass | — |
| library-0112-906a84cd5f67 | 千夏/手机.pmx | BlackPink - So Hot/luka.vmd | Not comparable | compare-fields |
| library-0113-ec1f1795f13f | 兹白_by_原神_cadf9ff361243b958d2a3571e3ec2dea/兹白.pmx | 144_キレキャリオンモーション/キレキャリオンモーション/キレキャリオン（左）.vmd | Preparation failed | prepare |
| library-0114-2aceb928f6c4 | xzy_MMD/darkstar/darkstar_MMD.pmx | 6666AAPのモーション素材集vol.01 歩き/Motion/段 シンプルウォーク.vmd | Recording failed | record |
| library-0115-bb323b235867 | xzy_MMD/aida/aida_MMD（武器).pmx | qyds0814_by_临时映画_eba9207c5b506c4f6b5f1ec7cbd1a361.vmd | Preparation failed | prepare |
| library-0116-8f23d1ee496e | EL-FPr FTB _ 双葉湊音2.0/双葉湊音2.0(二次創作モデル)_青春版.pmx | 104_えれくとりっく・えんじぇぅ/えれくとりっく・えんじぇぅ/electric_angel (penta).vmd | Over threshold | threshold |
| library-0117-e3e47d658c41 | Sakura (Dorm)/GirlsFrontline SakuraRestroom.pmx | MikuMambo_motion/ミクマンボ.vmd | Preparation failed | prepare |
| library-0118-3ff4753645ad | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/Ver.アニメ/ぽんぷ長式大和＿アニメ1.pmx | 兰音Reine-天兰夜-舞蹈数据_by_兰音Reine_efc44704ad051988b0a958abcf81cc43/天兰夜-motion数据-配布用-by：兰音Reine.vmd | Preparation failed | prepare |
| library-0119-4b0a4d7942ba | Helen (Starlit Waltz)/GirlsFrontline HelenSSR0101.pmx | IA INTERGALACTIA モーション/IA_INTERGALACTIA_full_key_version_FK.vmd | Preparation failed | prepare |
| library-0120-d87b798a435e | xzy_MMD/rasiel/rasiel_MMD(武器）.pmx | p_どっちにするの/motion_L.vmd | Preparation failed | prepare |
| library-0121-7079b0efa725 | 尼可_by_原神/笔.pmx | フランケンシュタインの怪物 1番サビ モーション/FrankensteinNoKaibutsu_Motion(MMD)_1Sabi_HinaSuzuki_v1.0[f1794].vmd | Preparation failed | prepare |
| library-0122-071c151fdff1 | 莱娅钻石之花/GirlsFrontline Leva1stGeneration.pmx | 099_Tell Your World モーション配布/Tell Your World モーション配布/Tell Your World 足浮き補正A.vmd | Recording failed | record |
| library-0123-a56987a582fe | 维普蕾（闪耀心愿）/GirlsFrontline VepleyCommanderSkin.pmx | 神っぽいなmotion/神っぽいな.vmd | Preparation failed | prepare |
| library-0124-1942a428afc5 | Sakura (Default)/Weapon Gohei.pmx | Taste The Feeling/Motion.vmd | Over threshold | threshold |
| library-0125-2031b4076006 | 蕾米埃尔 3.1/蕾米埃尔-黑/蕾米埃尔-黑/蕾米埃尔·黑(手和胸部无碰撞）.pmx | 164_bibbidiba_FullVer/bibbidiba_FullVer/bibbidiba_FullMotion/bibbidibaFull_DanceMotion.vmd | Preparation failed | prepare |
| library-0126-df6ad9e58be1 | ぽんぷ長式大和v.2/ぽんぷ長式大和v.2/九一式徹甲弾.pmx | レモンメロンクッキー/Tda式ミクレモンメロンクッキー.vmd | Not comparable | compare-fields |
| library-0127-2c1d0ec69ea4 | YYB Kagamine Rin Len_10th/YYB Kagamine Rin_10th/YYB Kagamine Rin_10th_v1.0.pmx | レモンメロンクッキー/あにまさ式ミクレモンメロンクッキー.vmd | Over threshold | threshold |
| library-0128-acfd2a8576df | Sharkry (Default)/GirlsFrontline SharkryDefault.pmx | IA HIGHER モーション/IA_HIGHER_light_version.vmd | Preparation failed | prepare |

</details>

## Raw artifact retention

Raw PMM, JSONL, and log artifacts are not retained by this quality-report workflow.
Snapshot retention flag: `false`.
