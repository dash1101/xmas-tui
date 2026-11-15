package io.github.dash1101.xmastui.core;

import java.util.ArrayList;
import java.util.List;

/** Speech-bubble text: wrapped to short lines (phones are narrow). */
public final class Bubbles {
    private Bubbles() {}

    public static String[] wrap(String text, int width) {
        List<String> out = new ArrayList<>();
        StringBuilder line = new StringBuilder();
        for (String word : text.split(" ")) {
            if (line.length() > 0 && line.length() + 1 + word.length() > width) {
                out.add(line.toString());
                line.setLength(0);
            }
            if (line.length() > 0) line.append(' ');
            line.append(word);
        }
        if (line.length() > 0) out.add(line.toString());
        return out.toArray(new String[0]);
    }
}
