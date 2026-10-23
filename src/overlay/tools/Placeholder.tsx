import type { OverlayToolProps } from '../overlayTypes'
import { TOOL_META } from './toolMeta'
import { TOOL_ICONS } from '../icons'

// Henüz yazılmamış araçlar için yer tutucu: sonraki fazlarda gerçek bileşenle değişir.
export default function PlaceholderTool({ id, props }: { id: keyof typeof TOOL_META; props: OverlayToolProps }) {
  const Icon = TOOL_ICONS[id]
  if (props.minimized) return null
  return <div className="ow-placeholder">
    <Icon size={28} />
    <b>{TOOL_META[id].title}</b>
    <p>{TOOL_META[id].title} bir sonraki fazda burada açılacak.</p>
  </div>
}
export const makePlaceholder = (id: keyof typeof TOOL_META) => function PlaceholderWindow(props: OverlayToolProps) { return <PlaceholderTool id={id} props={props} /> }
