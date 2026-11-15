import io.github.dash1101.xmastui.core.Config;
import io.github.dash1101.xmastui.core.Pix;
import io.github.dash1101.xmastui.core.Village;

import java.awt.*;
import java.awt.image.BufferedImage;
import java.io.File;
import javax.imageio.ImageIO;

/**
 * Desktop preview of the phone wallpaper (no Android needed): simulates the
 * village for a while and writes PNGs drawn the way the wallpaper draws them.
 * java Preview OUT_DIR [screenW screenH pixel seconds frames time dockStyle]
 */
public class Preview {
    public static void main(String[] a) throws Exception {
        File out = new File(a.length > 0 ? a[0] : "preview");
        out.mkdirs();
        int sw = a.length > 1 ? Integer.parseInt(a[1]) : 1080, sh = a.length > 2 ? Integer.parseInt(a[2]) : 2400;
        Config cfg = new Config();
        cfg.detail = a.length > 3 ? Integer.parseInt(a[3]) : 155;
        float secs = a.length > 4 ? Float.parseFloat(a[4]) : 30f;
        int frames = a.length > 5 ? Integer.parseInt(a[5]) : 1;
        cfg.time = a.length > 6 ? a[6] : "night";
        cfg.dockStyle = a.length > 7 ? a[7] : "frost";
        cfg.name = "Parker";
        cfg.petName = "Kitters";
        int scale = Math.max(3, Math.round(sw / (float) cfg.detail));
        int w = (sw + scale - 1) / scale, h = (sh + scale - 1) / scale;
        Village v = new Village(w, h, cfg, 7);
        Pix pix = new Pix(w, h);
        float dt = 1f / cfg.fps;
        for (float s = 0; s < secs; s += dt) v.update(dt);
        for (int i = 0; i < frames; i++) {
            for (int k = 0; k < (int) (cfg.fps * 1.5f); k++) v.update(dt);
            v.render(pix);
            BufferedImage img = new BufferedImage(w * scale, h * scale, BufferedImage.TYPE_INT_RGB);
            for (int y = 0; y < h; y++)
                for (int x = 0; x < w; x++) {
                    int c = pix.p[y * w + x];
                    for (int dy = 0; dy < scale; dy++)
                        for (int dx = 0; dx < scale; dx++) img.setRGB(x * scale + dx, y * scale + dy, c);
                }
            Graphics2D g = img.createGraphics();
            g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
            float font = Math.max(18f, sw / 34f);
            g.setFont(new Font(Font.MONOSPACED, Font.PLAIN, Math.round(font)));
            FontMetrics fm = g.getFontMetrics();
            for (Village.Say s : v.bubbles()) {
                String[] lines = io.github.dash1101.xmastui.core.Bubbles.wrap(s.text, 20);
                int tw = 0;
                for (String l : lines) tw = Math.max(tw, fm.stringWidth(l));
                int lh = fm.getHeight(), pad = Math.round(font * 0.45f);
                int bw = tw + pad * 2, bh = lh * lines.length + pad;
                int cx = Math.round(s.x * scale), by = Math.round(s.y * scale) - Math.round(font * 0.6f);
                int x0 = Math.max(6, Math.min(sw - bw - 6, cx - bw / 2)), y0 = by - bh;
                int al = Math.round(235 * s.alpha);
                g.setColor(new Color(244, 239, 230, al));
                g.fillRoundRect(x0, y0, bw, bh, pad * 2, pad * 2);
                g.fillPolygon(new int[] {cx - pad / 2, cx + pad / 2, cx}, new int[] {y0 + bh - 1, y0 + bh - 1, y0 + bh + pad}, 3);
                g.setColor(new Color(42, 26, 20, Math.round(255 * s.alpha)));
                for (int k = 0; k < lines.length; k++) g.drawString(lines[k], x0 + pad, y0 + pad / 2 + fm.getAscent() + k * lh);
            }
            g.dispose();
            ImageIO.write(img, "png", new File(out, String.format("frame_%02d.png", i)));
        }
        System.out.println("wrote " + frames + " frame(s) " + w + "x" + h + " px x" + scale);
    }
}
