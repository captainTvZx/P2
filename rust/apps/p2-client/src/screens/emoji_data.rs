//! OTOMATİK ÜRETİLDİ — emoji-picker-react dist/data/emojis.json (Electron EmojiPanel ile aynı veri ve sıra).
//! Bayraklar yok (EmojiPanel.tsx bayrak kategorisini listelemez). Biçim: (kategori, emoji, adlar|…).

pub static EMOJIS: &[(u8, &str, &str)] = &[
    (0, "😀", "face|grin|grinning face"),
    (0, "😃", "face|open|mouth|smile|grinning face with big eyes"),
    (0, "😄", "eye|face|open|mouth|smile|grinning face with smiling eyes"),
    (0, "😁", "eye|face|grin|smile|beaming face with smiling eyes"),
    (0, "😆", "face|laugh|mouth|smile|satisfied|grinning squinting face"),
    (0, "😅", "cold|face|open|smile|sweat|grinning face with sweat"),
    (0, "🤣", "face|rofl|floor|laugh|rotfl|rolling|rolling on the floor laughing"),
    (0, "😂", "joy|face|tear|laugh|face with tears of joy"),
    (0, "🙂", "face|smile|slightly smiling face"),
    (0, "🙃", "face|upside down|upside down face"),
    (0, "🫠", "melt|liquid|dissolve|disappear|melting face"),
    (0, "😉", "face|wink|winking face"),
    (0, "😊", "eye|face|blush|smile|smiling face with smiling eyes"),
    (0, "😇", "face|halo|angel|fantasy|innocent|smiling face with halo"),
    (0, "🥰", "adore|crush|hearts|in love|smiling face with hearts"),
    (0, "😍", "eye|face|love|smile|smiling face with heart eyes"),
    (0, "🤩", "eyes|face|star|grinning|star struck"),
    (0, "😘", "face|kiss|face blowing a kiss"),
    (0, "😗", "face|kiss|kissing face"),
    (0, "☺️", "face|smile|relaxed|outlined|smiling face"),
    (0, "😚", "eye|face|kiss|closed|kissing face with closed eyes"),
    (0, "😙", "eye|face|kiss|smile|kissing face with smiling eyes"),
    (0, "🥲", "tear|proud|smiling|touched|grateful|relieved|smiling face with tear"),
    (0, "😋", "yum|face|smile|delicious|savouring|face savoring food"),
    (0, "😛", "face|tongue|face with tongue"),
    (0, "😜", "eye|face|joke|wink|tongue|winking face with tongue"),
    (0, "🤪", "eye|goofy|large|small|zany face"),
    (0, "😝", "eye|face|taste|tongue|horrible|squinting face with tongue"),
    (0, "🤑", "face|money|mouth|money mouth face"),
    (0, "🤗", "hug|face|hugging|open hands|smiling face|smiling face with open hands"),
    (0, "🤭", "whoops|face with hand over mouth"),
    (0, "🫢", "awe|scared|surprise|amazement|disbelief|embarrass|face with open eyes and hand over mouth"),
    (0, "🫣", "peep|stare|captivated|face with peeking eye"),
    (0, "🤫", "quiet|shush|shushing face"),
    (0, "🤔", "face|thinking|thinking face"),
    (0, "🫡", "ok|yes|sunny|salute|troops|saluting face"),
    (0, "🤐", "zip|face|mouth|zipper|zipper mouth face"),
    (0, "🤨", "skeptic|distrust|face with raised eyebrow"),
    (0, "😐", "meh|face|deadpan|neutral|neutral face"),
    (0, "😑", "meh|face|inexpressive|unexpressive|expressionless|expressionless face"),
    (0, "😶", "face|mouth|quiet|silent|face without mouth"),
    (0, "🫥", "hide|depressed|disappear|introvert|invisible|dotted line face"),
    (0, "😶‍🌫️", "absentminded|face in clouds|head in clouds|face in the fog"),
    (0, "😏", "face|smirk|smirking face"),
    (0, "😒", "face|unhappy|unamused|unamused face"),
    (0, "🙄", "eyes|face|eyeroll|rolling|face with rolling eyes"),
    (0, "😬", "face|grimace|grimacing face"),
    (0, "😮‍💨", "gasp|groan|exhale|relief|whisper|whistle|face exhaling"),
    (0, "🤥", "lie|face|pinocchio|lying face"),
    (0, "🫨", "face|shock|shaking|vibrate|earthquake|shaking face"),
    (0, "🙂‍↔️", "no|shake|head shaking horizontally"),
    (0, "🙂‍↕️", "nod|yes|head shaking vertically"),
    (0, "😌", "face|relieved|relieved face"),
    (0, "😔", "face|pensive|dejected|pensive face"),
    (0, "😪", "face|sleep|good night|sleepy face"),
    (0, "🤤", "face|drooling|drooling face"),
    (0, "😴", "zzz|face|sleep|good night|sleeping face"),
    (0, "😷", "cold|face|mask|sick|doctor|face with medical mask"),
    (0, "🤒", "ill|face|sick|thermometer|face with thermometer"),
    (0, "🤕", "face|hurt|injury|bandage|face with head bandage"),
    (0, "🤢", "face|vomit|nauseated|nauseated face"),
    (0, "🤮", "puke|sick|vomit|face vomiting"),
    (0, "🤧", "face|sneeze|gesundheit|sneezing face"),
    (0, "🥵", "hot|hot face|feverish|sweating|red faced|heat stroke"),
    (0, "🥶", "cold|icicles|freezing|cold face|frostbite|blue faced"),
    (0, "🥴", "dizzy|tipsy|woozy face|wavy mouth|intoxicated|uneven eyes"),
    (0, "😵", "dead|face|knocked out|crossed out eyes|face with crossed out eyes"),
    (0, "😵‍💫", "whoa|dizzy|spiral|trouble|hypnotized|face with spiral eyes"),
    (0, "🤯", "shocked|mind blown|exploding head"),
    (0, "🤠", "hat|face|cowboy|cowgirl|cowboy hat face"),
    (0, "🥳", "hat|horn|party|celebration|partying face"),
    (0, "🥸", "face|nose|glasses|disguise|incognito|disguised face"),
    (0, "😎", "sun|cool|face|bright|sunglasses|smiling face with sunglasses"),
    (0, "🤓", "face|geek|nerd|nerd face"),
    (0, "🧐", "face|stuffy|monocle|face with monocle"),
    (0, "😕", "meh|face|confused|confused face"),
    (0, "🫤", "meh|unsure|skeptical|disappointed|face with diagonal mouth"),
    (0, "😟", "face|worried|worried face"),
    (0, "🙁", "face|frown|slightly frowning face"),
    (0, "☹️", "face|frown|frowning face"),
    (0, "😮", "face|open|mouth|sympathy|face with open mouth"),
    (0, "😯", "face|hushed|stunned|surprised|hushed face"),
    (0, "😲", "face|shocked|totally|astonished|astonished face"),
    (0, "😳", "face|dazed|flushed|flushed face"),
    (0, "🥺", "mercy|begging|puppy eyes|pleading face"),
    (0, "🥹", "cry|sad|angry|proud|resist|face holding back tears"),
    (0, "😦", "face|open|frown|mouth|frowning face with open mouth"),
    (0, "😧", "face|anguished|anguished face"),
    (0, "😨", "face|fear|scared|fearful|fearful face"),
    (0, "😰", "blue|cold|face|sweat|rushed|anxious face with sweat"),
    (0, "😥", "face|whew|relieved|disappointed|sad but relieved face"),
    (0, "😢", "cry|sad|face|tear|crying face"),
    (0, "😭", "cry|sad|sob|face|tear|loudly crying face"),
    (0, "😱", "face|fear|munch|scared|scream|face screaming in fear"),
    (0, "😖", "face|confounded|confounded face"),
    (0, "😣", "face|persevere|persevering face"),
    (0, "😞", "face|disappointed|disappointed face"),
    (0, "😓", "cold|face|sweat|downcast face with sweat"),
    (0, "😩", "face|tired|weary|weary face"),
    (0, "😫", "face|tired|tired face"),
    (0, "🥱", "yawn|bored|tired|yawning face"),
    (0, "😤", "won|face|triumph|face with steam from nose"),
    (0, "😡", "mad|red|face|rage|angry|enraged|pouting|enraged face"),
    (0, "😠", "mad|face|anger|angry|angry face"),
    (0, "🤬", "swearing|face with symbols on mouth"),
    (0, "😈", "face|horns|smile|fantasy|fairy tale|smiling face with horns"),
    (0, "👿", "imp|face|demon|devil|fantasy|angry face with horns"),
    (0, "💀", "face|skull|death|monster|fairy tale"),
    (0, "☠️", "face|death|skull|monster|crossbones|skull and crossbones"),
    (0, "💩", "poo|dung|face|poop|monster|pile of poo"),
    (0, "🤡", "face|clown|clown face"),
    (0, "👹", "ogre|face|fantasy|monster|creature|fairy tale"),
    (0, "👺", "face|goblin|fantasy|monster|creature|fairy tale"),
    (0, "👻", "face|ghost|fantasy|monster|creature|fairy tale"),
    (0, "👽", "ufo|face|alien|fantasy|creature|extraterrestrial"),
    (0, "👾", "ufo|face|alien|monster|creature|alien monster|extraterrestrial"),
    (0, "🤖", "face|robot|monster"),
    (0, "😺", "cat|face|open|mouth|smile|grinning|grinning cat"),
    (0, "😸", "cat|eye|face|grin|smile|grinning cat with smiling eyes"),
    (0, "😹", "cat|joy|face|tear|cat with tears of joy"),
    (0, "😻", "cat|eye|face|love|heart|smile|smiling cat with heart eyes"),
    (0, "😼", "cat|wry|face|smile|ironic|cat with wry smile"),
    (0, "😽", "cat|eye|face|kiss|kissing cat"),
    (0, "🙀", "oh|cat|face|weary|weary cat|surprised"),
    (0, "😿", "cat|cry|sad|face|tear|crying cat"),
    (0, "😾", "cat|face|pouting|pouting cat"),
    (0, "🙈", "see|evil|face|monkey|forbidden|see no evil monkey"),
    (0, "🙉", "evil|face|hear|monkey|forbidden|hear no evil monkey"),
    (0, "🙊", "evil|face|speak|monkey|forbidden|speak no evil monkey"),
    (0, "💌", "love|mail|heart|letter|love letter"),
    (0, "💘", "arrow|cupid|heart with arrow"),
    (0, "💝", "ribbon|valentine|heart with ribbon"),
    (0, "💖", "excited|sparkle|sparkling heart"),
    (0, "💗", "pulse|excited|growing|nervous|growing heart"),
    (0, "💓", "beating|heartbeat|pulsating|beating heart"),
    (0, "💞", "revolving|revolving hearts"),
    (0, "💕", "love|two hearts"),
    (0, "💟", "heart|heart decoration"),
    (0, "❣️", "mark|exclamation|punctuation|heart exclamation"),
    (0, "💔", "break|broken|broken heart"),
    (0, "❤️‍🔥", "burn|love|lust|heart|sacred heart|heart on fire"),
    (0, "❤️‍🩹", "well|mending|healthier|improving|recovering|recuperating|mending heart"),
    (0, "❤️", "heart|red heart"),
    (0, "🩷", "cute|like|love|pink|heart|pink heart"),
    (0, "🧡", "orange|orange heart"),
    (0, "💛", "yellow|yellow heart"),
    (0, "💚", "green|green heart"),
    (0, "💙", "blue|blue heart"),
    (0, "🩵", "cyan|teal|heart|light blue|light blue heart"),
    (0, "💜", "purple|purple heart"),
    (0, "🤎", "brown|heart|brown heart"),
    (0, "🖤", "evil|black|wicked|black heart"),
    (0, "🩶", "gray|heart|slate|silver|grey heart"),
    (0, "🤍", "heart|white|white heart"),
    (0, "💋", "kiss|lips|kiss mark"),
    (0, "💯", "100|full|score|hundred|hundred points"),
    (0, "💢", "mad|angry|comic|anger symbol"),
    (0, "💥", "boom|comic|collision"),
    (0, "💫", "star|dizzy|comic"),
    (0, "💦", "comic|sweat|splashing|sweat droplets"),
    (0, "💨", "dash|comic|running|dashing away"),
    (0, "🕳️", "hole"),
    (0, "💬", "comic|bubble|dialog|speech|balloon|speech balloon"),
    (0, "👁️‍🗨️", "eye|bubble|speech|balloon|witness|eye in speech bubble"),
    (0, "🗨️", "bubble|dialog|speech|balloon|left speech bubble"),
    (0, "🗯️", "mad|angry|bubble|balloon|right anger bubble"),
    (0, "💭", "comic|bubble|balloon|thought|thought balloon"),
    (0, "💤", "ZZZ|zzz|comic|sleep|good night"),
    (0, "👋", "hand|wave|waving|waving hand"),
    (0, "🤚", "raised|backhand|raised back of hand"),
    (0, "🖐️", "hand|finger|splayed|hand with fingers splayed"),
    (0, "✋", "hand|high 5|high five|raised hand"),
    (0, "🖖", "hand|spock|finger|vulcan|vulcan salute"),
    (0, "🫱", "hand|right|rightward|rightwards hand"),
    (0, "🫲", "hand|left|leftward|leftwards hand"),
    (0, "🫳", "drop|shoo|dismiss|palm down hand"),
    (0, "🫴", "come|catch|offer|beckon|palm up hand"),
    (0, "🫷", "push|stop|wait|refuse|leftward|high five|leftwards pushing hand"),
    (0, "🫸", "push|stop|wait|refuse|high five|rightward|rightwards pushing hand"),
    (0, "👌", "ok|hand|OK hand"),
    (0, "🤌", "fingers|pinched|sarcastic|hand gesture|interrogation|pinched fingers"),
    (0, "🤏", "small amount|pinching hand"),
    (0, "✌️", "v|hand|victory|victory hand"),
    (0, "🤞", "hand|luck|cross|finger|crossed fingers"),
    (0, "🫰", "love|snap|heart|money|expensive|hand with index finger and thumb crossed"),
    (0, "🤟", "ily|hand|love you gesture"),
    (0, "🤘", "hand|horns|finger|rock on|sign of the horns"),
    (0, "🤙", "call|hand|shaka|hang loose|call me hand"),
    (0, "👈", "hand|index|point|finger|backhand|backhand index pointing left"),
    (0, "👉", "hand|index|point|finger|backhand|backhand index pointing right"),
    (0, "👆", "up|hand|point|finger|backhand|backhand index pointing up"),
    (0, "🖕", "hand|finger|middle finger"),
    (0, "👇", "down|hand|point|finger|backhand|backhand index pointing down"),
    (0, "☝️", "up|hand|index|point|finger|index pointing up"),
    (0, "🫵", "you|point|index pointing at the viewer"),
    (0, "👍", "+1|up|hand|thumb|thumbs up"),
    (0, "👎", " 1|down|hand|thumb|thumbs down"),
    (0, "✊", "fist|hand|punch|clenched|raised fist"),
    (0, "👊", "fist|hand|punch|clenched|oncoming fist"),
    (0, "🤛", "fist|leftwards|left facing fist"),
    (0, "🤜", "fist|rightwards|right facing fist"),
    (0, "👏", "clap|hand|clapping hands"),
    (0, "🙌", "hand|hooray|raised|gesture|celebration|raising hands"),
    (0, "🫶", "love|heart hands"),
    (0, "👐", "hand|open|open hands"),
    (0, "🤲", "prayer|palms up together"),
    (0, "🤝", "hand|shake|meeting|handshake|agreement"),
    (0, "🙏", "ask|hand|pray|high 5|please|thanks|high five|folded hands"),
    (0, "✍️", "hand|write|writing hand"),
    (0, "💅", "care|nail|polish|manicure|cosmetics|nail polish"),
    (0, "🤳", "phone|selfie|camera"),
    (0, "💪", "flex|comic|biceps|muscle|flexed biceps"),
    (0, "🦾", "prosthetic|accessibility|mechanical arm"),
    (0, "🦿", "prosthetic|accessibility|mechanical leg"),
    (0, "🦵", "leg|kick|limb"),
    (0, "🦶", "foot|kick|stomp"),
    (0, "👂", "ear|body"),
    (0, "🦻", "accessibility|hard of hearing|ear with hearing aid"),
    (0, "👃", "nose|body"),
    (0, "🧠", "brain|intelligent"),
    (0, "🫀", "heart|organ|pulse|anatomical|cardiology|anatomical heart"),
    (0, "🫁", "lungs|organ|breath|exhalation|inhalation|respiration"),
    (0, "🦷", "tooth|dentist"),
    (0, "🦴", "bone|skeleton"),
    (0, "👀", "eye|eyes|face"),
    (0, "👁️", "eye|body"),
    (0, "👅", "body|tongue"),
    (0, "👄", "lips|mouth"),
    (0, "🫦", "fear|anxious|nervous|worried|flirting|biting lip|uncomfortable"),
    (0, "👶", "baby|young"),
    (0, "🧒", "child|young|gender neutral|unspecified gender"),
    (0, "👦", "boy|young"),
    (0, "👧", "girl|virgo|young|zodiac"),
    (0, "🧑", "adult|person|gender neutral|unspecified gender"),
    (0, "👱", "hair|blond|person: blond hair|blond haired person"),
    (0, "👨", "man|adult"),
    (0, "🧔", "beard|person|person: beard"),
    (0, "🧔‍♂️", "man|beard|man: beard"),
    (0, "🧔‍♀️", "beard|woman|woman: beard"),
    (0, "👨‍🦰", "man|adult|red hair|man: red hair"),
    (0, "👨‍🦱", "man|adult|curly hair|man: curly hair"),
    (0, "👨‍🦳", "man|adult|white hair|man: white hair"),
    (0, "👨‍🦲", "man|bald|adult|man: bald"),
    (0, "👩", "woman|adult"),
    (0, "👩‍🦰", "adult|woman|red hair|woman: red hair"),
    (0, "🧑‍🦰", "adult|person|red hair|gender neutral|person: red hair|unspecified gender"),
    (0, "👩‍🦱", "adult|woman|curly hair|woman: curly hair"),
    (0, "🧑‍🦱", "adult|person|curly hair|gender neutral|person: curly hair|unspecified gender"),
    (0, "👩‍🦳", "adult|woman|white hair|woman: white hair"),
    (0, "🧑‍🦳", "adult|person|white hair|gender neutral|person: white hair|unspecified gender"),
    (0, "👩‍🦲", "bald|adult|woman|woman: bald"),
    (0, "🧑‍🦲", "bald|adult|person|person: bald|gender neutral|unspecified gender"),
    (0, "👱‍♀️", "hair|woman|blonde|woman: blond hair|blond haired woman"),
    (0, "👱‍♂️", "man|hair|blond|man: blond hair|blond haired man"),
    (0, "🧓", "old|adult|older person|gender neutral|unspecified gender"),
    (0, "👴", "man|old|adult|old man"),
    (0, "👵", "old|adult|woman|old woman"),
    (0, "🙍", "frown|gesture|person frowning"),
    (0, "🙍‍♂️", "man|gesture|frowning|man frowning"),
    (0, "🙍‍♀️", "woman|gesture|frowning|woman frowning"),
    (0, "🙎", "gesture|pouting|person pouting"),
    (0, "🙎‍♂️", "man|gesture|pouting|man pouting"),
    (0, "🙎‍♀️", "woman|gesture|pouting|woman pouting"),
    (0, "🙅", "hand|gesture|forbidden|prohibited|person gesturing NO|person gesturing no"),
    (0, "🙅‍♂️", "man|hand|gesture|forbidden|prohibited|man gesturing NO|man gesturing no"),
    (0, "🙅‍♀️", "hand|woman|gesture|forbidden|prohibited|woman gesturing NO|woman gesturing no"),
    (0, "🙆", "ok|hand|gesture|person gesturing OK|person gesturing ok"),
    (0, "🙆‍♂️", "ok|man|hand|gesture|man gesturing OK|man gesturing ok"),
    (0, "🙆‍♀️", "ok|hand|woman|gesture|woman gesturing OK|woman gesturing ok"),
    (0, "💁", "hand|help|sassy|tipping|information|person tipping hand"),
    (0, "💁‍♂️", "man|sassy|tipping hand|man tipping hand"),
    (0, "💁‍♀️", "sassy|woman|tipping hand|woman tipping hand"),
    (0, "🙋", "hand|happy|raised|gesture|person raising hand"),
    (0, "🙋‍♂️", "man|gesture|raising hand|man raising hand"),
    (0, "🙋‍♀️", "woman|gesture|raising hand|woman raising hand"),
    (0, "🧏", "ear|deaf|hear|deaf person|accessibility"),
    (0, "🧏‍♂️", "man|deaf|deaf man"),
    (0, "🧏‍♀️", "deaf|woman|deaf woman"),
    (0, "🙇", "bow|sorry|apology|gesture|person bowing"),
    (0, "🙇‍♂️", "man|favor|sorry|bowing|apology|gesture|man bowing"),
    (0, "🙇‍♀️", "favor|sorry|woman|bowing|apology|gesture|woman bowing"),
    (0, "🤦", "face|palm|disbelief|exasperation|person facepalming"),
    (0, "🤦‍♂️", "man|facepalm|disbelief|exasperation|man facepalming"),
    (0, "🤦‍♀️", "woman|facepalm|disbelief|exasperation|woman facepalming"),
    (0, "🤷", "doubt|shrug|ignorance|indifference|person shrugging"),
    (0, "🤷‍♂️", "man|doubt|shrug|ignorance|indifference|man shrugging"),
    (0, "🤷‍♀️", "doubt|shrug|woman|ignorance|indifference|woman shrugging"),
    (0, "🧑‍⚕️", "nurse|doctor|therapist|healthcare|health worker"),
    (0, "👨‍⚕️", "man|nurse|doctor|therapist|healthcare|man health worker"),
    (0, "👩‍⚕️", "nurse|woman|doctor|therapist|healthcare|woman health worker"),
    (0, "🧑‍🎓", "student|graduate"),
    (0, "👨‍🎓", "man|student|graduate|man student"),
    (0, "👩‍🎓", "woman|student|graduate|woman student"),
    (0, "🧑‍🏫", "teacher|lecturer|professor|instructor"),
    (0, "👨‍🏫", "man|teacher|lecturer|professor|instructor|man teacher"),
    (0, "👩‍🏫", "woman|teacher|lecturer|professor|instructor|woman teacher"),
    (0, "🧑‍⚖️", "law|judge|scales|justice"),
    (0, "👨‍⚖️", "law|man|judge|scales|justice|man judge"),
    (0, "👩‍⚖️", "law|judge|woman|scales|justice|woman judge"),
    (0, "🧑‍🌾", "farmer|rancher|gardener"),
    (0, "👨‍🌾", "man|farmer|rancher|gardener|man farmer"),
    (0, "👩‍🌾", "woman|farmer|rancher|gardener|woman farmer"),
    (0, "🧑‍🍳", "cook|chef"),
    (0, "👨‍🍳", "man|chef|cook|man cook"),
    (0, "👩‍🍳", "chef|cook|woman|woman cook"),
    (0, "🧑‍🔧", "plumber|mechanic|electrician|tradesperson"),
    (0, "👨‍🔧", "man|plumber|mechanic|electrician|man mechanic|tradesperson"),
    (0, "👩‍🔧", "woman|plumber|mechanic|electrician|tradesperson|woman mechanic"),
    (0, "🧑‍🏭", "worker|factory|assembly|industrial|factory worker"),
    (0, "👨‍🏭", "man|worker|factory|assembly|industrial|man factory worker"),
    (0, "👩‍🏭", "woman|worker|factory|assembly|industrial|woman factory worker"),
    (0, "🧑‍💼", "manager|business|architect|white collar|office worker"),
    (0, "👨‍💼", "man|manager|business|architect|white collar|man office worker"),
    (0, "👩‍💼", "woman|manager|business|architect|white collar|woman office worker"),
    (0, "🧑‍🔬", "chemist|engineer|scientist|biologist|physicist"),
    (0, "👨‍🔬", "man|chemist|engineer|biologist|physicist|scientist|man scientist"),
    (0, "👩‍🔬", "woman|chemist|engineer|biologist|physicist|scientist|woman scientist"),
    (0, "🧑‍💻", "coder|inventor|software|developer|technologist"),
    (0, "👨‍💻", "man|coder|inventor|software|developer|technologist|man technologist"),
    (0, "👩‍💻", "coder|woman|inventor|software|developer|technologist|woman technologist"),
    (0, "🧑‍🎤", "rock|star|actor|singer|entertainer"),
    (0, "👨‍🎤", "man|rock|star|actor|singer|man singer|entertainer"),
    (0, "👩‍🎤", "rock|star|actor|woman|singer|entertainer|woman singer"),
    (0, "🧑‍🎨", "artist|palette"),
    (0, "👨‍🎨", "man|artist|palette|man artist"),
    (0, "👩‍🎨", "woman|artist|palette|woman artist"),
    (0, "🧑‍✈️", "pilot|plane"),
    (0, "👨‍✈️", "man|pilot|plane|man pilot"),
    (0, "👩‍✈️", "pilot|plane|woman|woman pilot"),
    (0, "🧑‍🚀", "rocket|astronaut"),
    (0, "👨‍🚀", "man|rocket|astronaut|man astronaut"),
    (0, "👩‍🚀", "woman|rocket|astronaut|woman astronaut"),
    (0, "🧑‍🚒", "fire|firetruck|firefighter"),
    (0, "👨‍🚒", "man|firetruck|firefighter|man firefighter"),
    (0, "👩‍🚒", "woman|firetruck|firefighter|woman firefighter"),
    (0, "👮", "cop|police|officer|police officer"),
    (0, "👮‍♂️", "cop|man|police|officer|man police officer"),
    (0, "👮‍♀️", "cop|woman|police|officer|woman police officer"),
    (0, "🕵️", "spy|sleuth|detective"),
    (0, "🕵️‍♂️", "man|spy|sleuth|detective|man detective"),
    (0, "🕵️‍♀️", "spy|woman|sleuth|detective|woman detective"),
    (0, "💂", "guard"),
    (0, "💂‍♂️", "man|guard|man guard"),
    (0, "💂‍♀️", "guard|woman|woman guard"),
    (0, "🥷", "ninja|hidden|fighter|stealth"),
    (0, "👷", "hat|worker|construction|construction worker"),
    (0, "👷‍♂️", "man|worker|construction|man construction worker"),
    (0, "👷‍♀️", "woman|worker|construction|woman construction worker"),
    (0, "🫅", "noble|regal|monarch|royalty|person with crown"),
    (0, "🤴", "prince"),
    (0, "👸", "fantasy|princess|fairy tale"),
    (0, "👳", "turban|person wearing turban"),
    (0, "👳‍♂️", "man|turban|man wearing turban"),
    (0, "👳‍♀️", "woman|turban|woman wearing turban"),
    (0, "👲", "cap|hat|person|skullcap|gua pi mao|person with skullcap"),
    (0, "🧕", "hijab|tichel|mantilla|headscarf|woman with headscarf"),
    (0, "🤵", "groom|person|tuxedo|person in tuxedo"),
    (0, "🤵‍♂️", "man|tuxedo|man in tuxedo"),
    (0, "🤵‍♀️", "woman|tuxedo|woman in tuxedo"),
    (0, "👰", "veil|bride|person|wedding|person with veil"),
    (0, "👰‍♂️", "man|veil|man with veil"),
    (0, "👰‍♀️", "veil|woman|woman with veil"),
    (0, "🤰", "woman|pregnant|pregnant woman"),
    (0, "🫃", "full|belly|bloated|pregnant|pregnant man"),
    (0, "🫄", "full|belly|bloated|pregnant|pregnant person"),
    (0, "🤱", "baby|breast|nursing|breast feeding"),
    (0, "👩‍🍼", "baby|woman|feeding|nursing|woman feeding baby"),
    (0, "👨‍🍼", "man|baby|feeding|nursing|man feeding baby"),
    (0, "🧑‍🍼", "baby|person|feeding|nursing|person feeding baby"),
    (0, "👼", "baby|face|angel|fantasy|baby angel|fairy tale"),
    (0, "🎅", "claus|santa|father|christmas|Santa Claus|celebration"),
    (0, "🤶", "mrs.|claus|mother|christmas|Mrs. Claus|celebration"),
    (0, "🧑‍🎄", "claus|mx claus|christmas"),
    (0, "🦸", "good|hero|heroine|superhero|superpower"),
    (0, "🦸‍♂️", "man|good|hero|superpower|man superhero"),
    (0, "🦸‍♀️", "good|hero|woman|heroine|superpower|woman superhero"),
    (0, "🦹", "evil|villain|criminal|superpower|supervillain"),
    (0, "🦹‍♂️", "man|evil|villain|criminal|superpower|man supervillain"),
    (0, "🦹‍♀️", "evil|woman|villain|criminal|superpower|woman supervillain"),
    (0, "🧙", "mage|witch|wizard|sorcerer|sorceress"),
    (0, "🧙‍♂️", "wizard|man mage|sorcerer"),
    (0, "🧙‍♀️", "witch|sorceress|woman mage"),
    (0, "🧚", "puck|fairy|oberon|titania"),
    (0, "🧚‍♂️", "puck|oberon|man fairy"),
    (0, "🧚‍♀️", "titania|woman fairy"),
    (0, "🧛", "undead|vampire|dracula"),
    (0, "🧛‍♂️", "undead|dracula|man vampire"),
    (0, "🧛‍♀️", "undead|woman vampire"),
    (0, "🧜", "merman|mermaid|merwoman|merperson"),
    (0, "🧜‍♂️", "merman|triton"),
    (0, "🧜‍♀️", "mermaid|merwoman"),
    (0, "🧝", "elf|magical"),
    (0, "🧝‍♂️", "man elf|magical"),
    (0, "🧝‍♀️", "magical|woman elf"),
    (0, "🧞", "genie|djinn"),
    (0, "🧞‍♂️", "djinn|man genie"),
    (0, "🧞‍♀️", "djinn|woman genie"),
    (0, "🧟", "zombie|undead|walking dead"),
    (0, "🧟‍♂️", "undead|man zombie|walking dead"),
    (0, "🧟‍♀️", "undead|woman zombie|walking dead"),
    (0, "🧌", "troll|fantasy|monster|fairy tale"),
    (0, "💆", "face|salon|massage|person getting massage"),
    (0, "💆‍♂️", "man|face|massage|man getting massage"),
    (0, "💆‍♀️", "face|woman|massage|woman getting massage"),
    (0, "💇", "barber|beauty|parlor|haircut|person getting haircut"),
    (0, "💇‍♂️", "man|haircut|man getting haircut"),
    (0, "💇‍♀️", "woman|haircut|woman getting haircut"),
    (0, "🚶", "hike|walk|walking|person walking"),
    (0, "🚶‍♂️", "man|hike|walk|man walking"),
    (0, "🚶‍♀️", "hike|walk|woman|woman walking"),
    (0, "🚶‍➡️", "hike|walk|walking|person walking|person walking facing right"),
    (0, "🚶‍♀️‍➡️", "hike|walk|woman|woman walking|woman walking facing right"),
    (0, "🚶‍♂️‍➡️", "man|hike|walk|man walking|man walking facing right"),
    (0, "🧍", "stand|standing|person standing"),
    (0, "🧍‍♂️", "man|standing|man standing"),
    (0, "🧍‍♀️", "woman|standing|woman standing"),
    (0, "🧎", "kneel|kneeling|person kneeling"),
    (0, "🧎‍♂️", "man|kneeling|man kneeling"),
    (0, "🧎‍♀️", "woman|kneeling|woman kneeling"),
    (0, "🧎‍➡️", "kneel|kneeling|person kneeling|person kneeling facing right"),
    (0, "🧎‍♀️‍➡️", "woman|kneeling|woman kneeling facing right"),
    (0, "🧎‍♂️‍➡️", "man|kneeling|man kneeling facing right"),
    (0, "🧑‍🦯", "blind|accessibility|person with white cane"),
    (0, "🧑‍🦯‍➡️", "blind|accessibility|person with white cane|person with white cane facing right"),
    (0, "👨‍🦯", "man|blind|accessibility|man with white cane"),
    (0, "👨‍🦯‍➡️", "man|blind|accessibility|man with white cane|man with white cane facing right"),
    (0, "👩‍🦯", "blind|woman|accessibility|woman with white cane"),
    (0, "👩‍🦯‍➡️", "blind|woman|accessibility|woman with white cane|woman with white cane facing right"),
    (0, "🧑‍🦼", "wheelchair|accessibility|person in motorized wheelchair"),
    (0, "🧑‍🦼‍➡️", "wheelchair|accessibility|person in motorized wheelchair|person in motorized wheelchair facing right"),
    (0, "👨‍🦼", "man|wheelchair|accessibility|man in motorized wheelchair"),
    (0, "👨‍🦼‍➡️", "man|wheelchair|accessibility|man in motorized wheelchair|man in motorized wheelchair facing right"),
    (0, "👩‍🦼", "woman|wheelchair|accessibility|woman in motorized wheelchair"),
    (0, "👩‍🦼‍➡️", "woman|wheelchair|accessibility|woman in motorized wheelchair|woman in motorized wheelchair facing right"),
    (0, "🧑‍🦽", "wheelchair|accessibility|person in manual wheelchair"),
    (0, "🧑‍🦽‍➡️", "wheelchair|accessibility|person in manual wheelchair|person in manual wheelchair facing right"),
    (0, "👨‍🦽", "man|wheelchair|accessibility|man in manual wheelchair"),
    (0, "👨‍🦽‍➡️", "man|wheelchair|accessibility|man in manual wheelchair|man in manual wheelchair facing right"),
    (0, "👩‍🦽", "woman|wheelchair|accessibility|woman in manual wheelchair"),
    (0, "👩‍🦽‍➡️", "woman|wheelchair|accessibility|woman in manual wheelchair|woman in manual wheelchair facing right"),
    (0, "🏃", "running|marathon|person running"),
    (0, "🏃‍♂️", "man|racing|running|marathon|man running"),
    (0, "🏃‍♀️", "woman|racing|running|marathon|woman running"),
    (0, "🏃‍➡️", "running|marathon|person running|person running facing right"),
    (0, "🏃‍♀️‍➡️", "woman|racing|running|marathon|woman running facing right"),
    (0, "🏃‍♂️‍➡️", "man|racing|running|marathon|man running facing right"),
    (0, "💃", "dance|woman|dancing|woman dancing"),
    (0, "🕺", "man|dance|dancing|man dancing"),
    (0, "🕴️", "suit|person|business|person in suit levitating"),
    (0, "👯", "dancer|partying|bunny ear|people with bunny ears"),
    (0, "👯‍♂️", "men|dancer|partying|bunny ear|men with bunny ears"),
    (0, "👯‍♀️", "women|dancer|partying|bunny ear|women with bunny ears"),
    (0, "🧖", "sauna|steam room|person in steamy room"),
    (0, "🧖‍♂️", "sauna|steam room|man in steamy room"),
    (0, "🧖‍♀️", "sauna|steam room|woman in steamy room"),
    (0, "🧗", "climber|person climbing"),
    (0, "🧗‍♂️", "climber|man climbing"),
    (0, "🧗‍♀️", "climber|woman climbing"),
    (0, "🤺", "sword|fencer|fencing|person fencing"),
    (0, "🏇", "horse|jockey|racing|racehorse|horse racing"),
    (0, "⛷️", "ski|snow|skier"),
    (0, "🏂", "ski|snow|snowboard|snowboarder"),
    (0, "🏌️", "ball|golf|person golfing"),
    (0, "🏌️‍♂️", "man|golf|man golfing"),
    (0, "🏌️‍♀️", "golf|woman|woman golfing"),
    (0, "🏄", "surfing|person surfing"),
    (0, "🏄‍♂️", "man|surfing|man surfing"),
    (0, "🏄‍♀️", "woman|surfing|woman surfing"),
    (0, "🚣", "boat|rowboat|person rowing boat"),
    (0, "🚣‍♂️", "man|boat|rowboat|man rowing boat"),
    (0, "🚣‍♀️", "boat|woman|rowboat|woman rowing boat"),
    (0, "🏊", "swim|person swimming"),
    (0, "🏊‍♂️", "man|swim|man swimming"),
    (0, "🏊‍♀️", "swim|woman|woman swimming"),
    (0, "⛹️", "ball|person bouncing ball"),
    (0, "⛹️‍♂️", "man|ball|man bouncing ball"),
    (0, "⛹️‍♀️", "ball|woman|woman bouncing ball"),
    (0, "🏋️", "lifter|weight|person lifting weights"),
    (0, "🏋️‍♂️", "man|weight lifter|man lifting weights"),
    (0, "🏋️‍♀️", "woman|weight lifter|woman lifting weights"),
    (0, "🚴", "biking|bicycle|cyclist|person biking"),
    (0, "🚴‍♂️", "man|biking|bicycle|cyclist|man biking"),
    (0, "🚴‍♀️", "woman|biking|bicycle|cyclist|woman biking"),
    (0, "🚵", "bike|bicycle|cyclist|mountain|bicyclist|person mountain biking"),
    (0, "🚵‍♂️", "man|bike|bicycle|cyclist|mountain|man mountain biking"),
    (0, "🚵‍♀️", "bike|woman|biking|bicycle|cyclist|mountain|woman mountain biking"),
    (0, "🤸", "cartwheel|gymnastics|person cartwheeling"),
    (0, "🤸‍♂️", "man|cartwheel|gymnastics|man cartwheeling"),
    (0, "🤸‍♀️", "woman|cartwheel|gymnastics|woman cartwheeling"),
    (0, "🤼", "wrestle|wrestler|people wrestling"),
    (0, "🤼‍♂️", "men|wrestle|men wrestling"),
    (0, "🤼‍♀️", "women|wrestle|women wrestling"),
    (0, "🤽", "polo|water|person playing water polo"),
    (0, "🤽‍♂️", "man|water polo|man playing water polo"),
    (0, "🤽‍♀️", "woman|water polo|woman playing water polo"),
    (0, "🤾", "ball|handball|person playing handball"),
    (0, "🤾‍♂️", "man|handball|man playing handball"),
    (0, "🤾‍♀️", "woman|handball|woman playing handball"),
    (0, "🤹", "skill|juggle|balance|multitask|person juggling"),
    (0, "🤹‍♂️", "man|juggling|multitask|man juggling"),
    (0, "🤹‍♀️", "woman|juggling|multitask|woman juggling"),
    (0, "🧘", "yoga|meditation|person in lotus position"),
    (0, "🧘‍♂️", "yoga|meditation|man in lotus position"),
    (0, "🧘‍♀️", "yoga|meditation|woman in lotus position"),
    (0, "🛀", "bath|bathtub|person taking bath"),
    (0, "🛌", "hotel|sleep|good night|person in bed"),
    (0, "🧑‍🤝‍🧑", "hand|hold|couple|person|holding hands|people holding hands"),
    (0, "👭", "hand|women|couple|holding hands|women holding hands"),
    (0, "👫", "man|hand|hold|woman|couple|holding hands|woman and man holding hands"),
    (0, "👬", "man|men|twins|couple|gemini|zodiac|holding hands|men holding hands"),
    (0, "💏", "kiss|couple"),
    (0, "👩‍❤️‍💋‍👨", "man|kiss|woman|couple|kiss: woman, man"),
    (0, "👨‍❤️‍💋‍👨", "man|kiss|couple|kiss: man, man"),
    (0, "👩‍❤️‍💋‍👩", "kiss|woman|couple|kiss: woman, woman"),
    (0, "💑", "love|couple|couple with heart"),
    (0, "👩‍❤️‍👨", "man|love|woman|couple|couple with heart|couple with heart: woman, man"),
    (0, "👨‍❤️‍👨", "man|love|couple|couple with heart|couple with heart: man, man"),
    (0, "👩‍❤️‍👩", "love|woman|couple|couple with heart|couple with heart: woman, woman"),
    (0, "👨‍👩‍👦", "boy|man|woman|family|family: man, woman, boy"),
    (0, "👨‍👩‍👧", "man|girl|woman|family|family: man, woman, girl"),
    (0, "👨‍👩‍👧‍👦", "boy|man|girl|woman|family|family: man, woman, girl, boy"),
    (0, "👨‍👩‍👦‍👦", "boy|man|woman|family|family: man, woman, boy, boy"),
    (0, "👨‍👩‍👧‍👧", "man|girl|woman|family|family: man, woman, girl, girl"),
    (0, "👨‍👨‍👦", "boy|man|family|family: man, man, boy"),
    (0, "👨‍👨‍👧", "man|girl|family|family: man, man, girl"),
    (0, "👨‍👨‍👧‍👦", "boy|man|girl|family|family: man, man, girl, boy"),
    (0, "👨‍👨‍👦‍👦", "boy|man|family|family: man, man, boy, boy"),
    (0, "👨‍👨‍👧‍👧", "man|girl|family|family: man, man, girl, girl"),
    (0, "👩‍👩‍👦", "boy|woman|family|family: woman, woman, boy"),
    (0, "👩‍👩‍👧", "girl|woman|family|family: woman, woman, girl"),
    (0, "👩‍👩‍👧‍👦", "boy|girl|woman|family|family: woman, woman, girl, boy"),
    (0, "👩‍👩‍👦‍👦", "boy|woman|family|family: woman, woman, boy, boy"),
    (0, "👩‍👩‍👧‍👧", "girl|woman|family|family: woman, woman, girl, girl"),
    (0, "👨‍👦", "boy|man|family|family: man, boy"),
    (0, "👨‍👦‍👦", "boy|man|family|family: man, boy, boy"),
    (0, "👨‍👧", "man|girl|family|family: man, girl"),
    (0, "👨‍👧‍👦", "boy|man|girl|family|family: man, girl, boy"),
    (0, "👨‍👧‍👧", "man|girl|family|family: man, girl, girl"),
    (0, "👩‍👦", "boy|woman|family|family: woman, boy"),
    (0, "👩‍👦‍👦", "boy|woman|family|family: woman, boy, boy"),
    (0, "👩‍👧", "girl|woman|family|family: woman, girl"),
    (0, "👩‍👧‍👦", "boy|girl|woman|family|family: woman, girl, boy"),
    (0, "👩‍👧‍👧", "girl|woman|family|family: woman, girl, girl"),
    (0, "🗣️", "face|head|speak|speaking|silhouette|speaking head"),
    (0, "👤", "bust|silhouette|bust in silhouette"),
    (0, "👥", "bust|silhouette|busts in silhouette"),
    (0, "🫂", "hug|hello|thanks|goodbye|people hugging"),
    (0, "👪", "family"),
    (0, "🧑‍🧑‍🧒", "family: adult, adult, child"),
    (0, "🧑‍🧑‍🧒‍🧒", "family: adult, adult, child, child"),
    (0, "🧑‍🧒", "family: adult, child"),
    (0, "🧑‍🧒‍🧒", "family: adult, child, child"),
    (0, "👣", "print|clothing|footprint|footprints"),
    (1, "🐵", "face|monkey|monkey face"),
    (1, "🐒", "monkey"),
    (1, "🦍", "gorilla"),
    (1, "🦧", "ape|orangutan"),
    (1, "🐶", "dog|pet|face|dog face"),
    (1, "🐕", "dog|pet"),
    (1, "🦮", "blind|guide|guide dog|accessibility"),
    (1, "🐕‍🦺", "dog|service|assistance|service dog|accessibility"),
    (1, "🐩", "dog|poodle"),
    (1, "🐺", "wolf|face"),
    (1, "🦊", "fox|face"),
    (1, "🦝", "sly|raccoon|curious"),
    (1, "🐱", "cat|pet|face|cat face"),
    (1, "🐈", "cat|pet"),
    (1, "🐈‍⬛", "cat|black|unlucky|black cat"),
    (1, "🦁", "leo|lion|face|zodiac"),
    (1, "🐯", "face|tiger|tiger face"),
    (1, "🐅", "tiger"),
    (1, "🐆", "leopard"),
    (1, "🐴", "face|horse|horse face"),
    (1, "🫎", "elk|moose|animal|mammal|antlers"),
    (1, "🫏", "ass|mule|burro|donkey|animal|mammal|stubborn"),
    (1, "🐎", "horse|racing|racehorse|equestrian"),
    (1, "🦄", "face|unicorn"),
    (1, "🦓", "zebra|stripe"),
    (1, "🦌", "deer"),
    (1, "🦬", "herd|bison|wisent|buffalo"),
    (1, "🐮", "cow|face|cow face"),
    (1, "🐂", "ox|bull|taurus|zodiac"),
    (1, "🐃", "water|buffalo|water buffalo"),
    (1, "🐄", "cow"),
    (1, "🐷", "pig|face|pig face"),
    (1, "🐖", "pig|sow"),
    (1, "🐗", "pig|boar"),
    (1, "🐽", "pig|face|nose|pig nose"),
    (1, "🐏", "ram|male|aries|sheep|zodiac"),
    (1, "🐑", "ewe|sheep|female"),
    (1, "🐐", "goat|zodiac|capricorn"),
    (1, "🐪", "hump|camel|dromedary"),
    (1, "🐫", "hump|camel|bactrian|two hump camel"),
    (1, "🦙", "wool|llama|alpaca|vicuña|guanaco"),
    (1, "🦒", "spots|giraffe"),
    (1, "🐘", "elephant"),
    (1, "🦣", "tusk|large|woolly|mammoth|extinction"),
    (1, "🦏", "rhinoceros"),
    (1, "🦛", "hippo|hippopotamus"),
    (1, "🐭", "face|mouse|mouse face"),
    (1, "🐁", "mouse"),
    (1, "🐀", "rat"),
    (1, "🐹", "pet|face|hamster"),
    (1, "🐰", "pet|face|bunny|rabbit|rabbit face"),
    (1, "🐇", "pet|bunny|rabbit"),
    (1, "🐿️", "chipmunk|squirrel"),
    (1, "🦫", "dam|beaver"),
    (1, "🦔", "spiny|hedgehog"),
    (1, "🦇", "bat|vampire"),
    (1, "🐻", "bear|face"),
    (1, "🐻‍❄️", "bear|white|arctic|polar bear"),
    (1, "🐨", "face|koala|marsupial"),
    (1, "🐼", "face|panda"),
    (1, "🦥", "lazy|slow|sloth"),
    (1, "🦦", "otter|fishing|playful"),
    (1, "🦨", "skunk|stink"),
    (1, "🦘", "joey|jump|kangaroo|marsupial"),
    (1, "🦡", "badger|pester|honey badger"),
    (1, "🐾", "paw|feet|print|paw prints"),
    (1, "🦃", "bird|turkey"),
    (1, "🐔", "bird|chicken"),
    (1, "🐓", "bird|rooster"),
    (1, "🐣", "baby|bird|chick|hatching|hatching chick"),
    (1, "🐤", "baby|bird|chick|baby chick"),
    (1, "🐥", "baby|bird|chick|front facing baby chick"),
    (1, "🐦", "bird"),
    (1, "🐧", "bird|penguin"),
    (1, "🕊️", "fly|dove|bird|peace"),
    (1, "🦅", "bird|eagle"),
    (1, "🦆", "duck|bird"),
    (1, "🦢", "swan|bird|cygnet|ugly duckling"),
    (1, "🦉", "owl|bird|wise"),
    (1, "🦤", "dodo|large|mauritius|extinction"),
    (1, "🪶", "bird|light|flight|feather|plumage"),
    (1, "🦩", "flamingo|tropical|flamboyant"),
    (1, "🦚", "bird|proud|peahen|peacock|ostentatious"),
    (1, "🦜", "bird|talk|parrot|pirate"),
    (1, "🪽", "wing|bird|flying|angelic|aviation|mythology"),
    (1, "🐦‍⬛", "bird|crow|rook|black|raven|black bird"),
    (1, "🪿", "bird|fowl|honk|goose|silly"),
    (1, "🐦‍🔥", "phoenix|fantasy|rebirth|firebird|reincarnation"),
    (1, "🐸", "frog|face"),
    (1, "🐊", "crocodile"),
    (1, "🐢", "turtle|terrapin|tortoise"),
    (1, "🦎", "lizard|reptile"),
    (1, "🐍", "snake|bearer|zodiac|serpent|ophiuchus"),
    (1, "🐲", "face|dragon|fairy tale|dragon face"),
    (1, "🐉", "dragon|fairy tale"),
    (1, "🦕", "sauropod|diplodocus|brontosaurus|brachiosaurus"),
    (1, "🦖", "T Rex|t rex|tyrannosaurus rex"),
    (1, "🐳", "face|whale|spouting|spouting whale"),
    (1, "🐋", "whale"),
    (1, "🐬", "dolphin|flipper"),
    (1, "🦭", "seal|sea lion"),
    (1, "🐟", "fish|pisces|zodiac"),
    (1, "🐠", "fish|tropical|tropical fish"),
    (1, "🐡", "fish|blowfish"),
    (1, "🦈", "fish|shark"),
    (1, "🐙", "octopus"),
    (1, "🐚", "shell|spiral|spiral shell"),
    (1, "🪸", "reef|coral|ocean"),
    (1, "🪼", "burn|ouch|jelly|marine|stinger|jellyfish|invertebrate"),
    (1, "🐌", "snail"),
    (1, "🦋", "insect|pretty|butterfly"),
    (1, "🐛", "bug|insect"),
    (1, "🐜", "ant|insect"),
    (1, "🐝", "bee|insect|honeybee"),
    (1, "🪲", "bug|beetle|insect"),
    (1, "🐞", "beetle|insect|ladybug|ladybird|lady beetle"),
    (1, "🦗", "cricket|grasshopper"),
    (1, "🪳", "pest|roach|insect|cockroach"),
    (1, "🕷️", "spider|insect"),
    (1, "🕸️", "web|spider|spider web"),
    (1, "🦂", "zodiac|scorpio|scorpion"),
    (1, "🦟", "pest|fever|virus|disease|malaria|mosquito"),
    (1, "🪰", "fly|pest|maggot|disease|rotting"),
    (1, "🪱", "worm|annelid|parasite|earthworm"),
    (1, "🦠", "virus|amoeba|microbe|bacteria"),
    (1, "💐", "flower|bouquet"),
    (1, "🌸", "cherry|flower|blossom|cherry blossom"),
    (1, "💮", "flower|white flower"),
    (1, "🪷", "lotus|flower|purity|buddhism|hinduism"),
    (1, "🏵️", "plant|rosette"),
    (1, "🌹", "rose|flower"),
    (1, "🥀", "flower|wilted|wilted flower"),
    (1, "🌺", "flower|hibiscus"),
    (1, "🌻", "sun|flower|sunflower"),
    (1, "🌼", "flower|blossom"),
    (1, "🌷", "tulip|flower"),
    (1, "🪻", "flower|lupine|hyacinth|lavender|bluebonnet|snapdragon"),
    (1, "🌱", "young|seedling"),
    (1, "🪴", "grow|house|plant|boring|useless|nurturing|potted plant"),
    (1, "🌲", "tree|evergreen tree"),
    (1, "🌳", "tree|shedding|deciduous|deciduous tree"),
    (1, "🌴", "palm|tree|palm tree"),
    (1, "🌵", "plant|cactus"),
    (1, "🌾", "ear|rice|grain|sheaf of rice"),
    (1, "🌿", "herb|leaf"),
    (1, "☘️", "plant|shamrock"),
    (1, "🍀", "4|four|leaf|clover|four leaf clover"),
    (1, "🍁", "leaf|maple|falling|maple leaf"),
    (1, "🍂", "leaf|falling|fallen leaf"),
    (1, "🍃", "blow|leaf|wind|flutter|leaf fluttering in wind"),
    (1, "🪹", "nesting|empty nest"),
    (1, "🪺", "nesting|nest with eggs"),
    (1, "🍄", "mushroom|toadstool"),
    (2, "🍇", "fruit|grape|grapes"),
    (2, "🍈", "melon|fruit"),
    (2, "🍉", "fruit|watermelon"),
    (2, "🍊", "fruit|orange|tangerine"),
    (2, "🍋", "lemon|fruit|citrus"),
    (2, "🍋‍🟩", "lime|fruit|citrus|tropical"),
    (2, "🍌", "fruit|banana"),
    (2, "🍍", "fruit|pineapple"),
    (2, "🥭", "mango|fruit|tropical"),
    (2, "🍎", "red|apple|fruit|red apple"),
    (2, "🍏", "apple|fruit|green|green apple"),
    (2, "🍐", "pear|fruit"),
    (2, "🍑", "peach|fruit"),
    (2, "🍒", "red|fruit|cherry|berries|cherries"),
    (2, "🍓", "berry|fruit|strawberry"),
    (2, "🫐", "blue|berry|bilberry|blueberry|blueberries"),
    (2, "🥝", "food|kiwi|fruit|kiwi fruit"),
    (2, "🍅", "fruit|tomato|vegetable"),
    (2, "🫒", "food|olive"),
    (2, "🥥", "palm|coconut|piña colada"),
    (2, "🥑", "food|fruit|avocado"),
    (2, "🍆", "eggplant|aubergine|vegetable"),
    (2, "🥔", "food|potato|vegetable"),
    (2, "🥕", "food|carrot|vegetable"),
    (2, "🌽", "ear|corn|maze|maize|ear of corn"),
    (2, "🌶️", "hot|pepper|hot pepper"),
    (2, "🫑", "pepper|capsicum|vegetable|bell pepper"),
    (2, "🥒", "food|pickle|cucumber|vegetable"),
    (2, "🥬", "kale|cabbage|lettuce|bok choy|leafy green"),
    (2, "🥦", "broccoli|wild cabbage"),
    (2, "🧄", "garlic|flavoring"),
    (2, "🧅", "onion|flavoring"),
    (2, "🥜", "nut|food|peanut|peanuts|vegetable"),
    (2, "🫘", "food|beans|kidney|legume"),
    (2, "🌰", "plant|chestnut"),
    (2, "🫚", "beer|root|spice|ginger root"),
    (2, "🫛", "pea|pod|beans|legume|pea pod|edamame|vegetable"),
    (2, "🍄‍🟫", "food|fungus|nature|vegetable|brown mushroom"),
    (2, "🍞", "loaf|bread"),
    (2, "🥐", "food|roll|bread|french|croissant|breakfast"),
    (2, "🥖", "food|bread|french|baguette|baguette bread"),
    (2, "🫓", "naan|pita|arepa|lavash|flatbread"),
    (2, "🥨", "pretzel|twisted"),
    (2, "🥯", "bagel|bakery|schmear|breakfast"),
    (2, "🥞", "food|crêpe|hotcake|pancake|pancakes|breakfast"),
    (2, "🧇", "iron|waffle|breakfast|indecisive"),
    (2, "🧀", "cheese|cheese wedge"),
    (2, "🍖", "bone|meat|meat on bone"),
    (2, "🍗", "leg|bone|chicken|poultry|drumstick|poultry leg"),
    (2, "🥩", "chop|steak|lambchop|porkchop|cut of meat"),
    (2, "🥓", "food|meat|bacon|breakfast"),
    (2, "🍔", "burger|hamburger"),
    (2, "🍟", "fries|french|french fries"),
    (2, "🍕", "pizza|slice|cheese"),
    (2, "🌭", "hotdog|hot dog|sausage|frankfurter"),
    (2, "🥪", "bread|sandwich"),
    (2, "🌮", "taco|mexican"),
    (2, "🌯", "wrap|burrito|mexican"),
    (2, "🫔", "tamale|mexican|wrapped"),
    (2, "🥙", "food|gyro|kebab|falafel|stuffed|flatbread|stuffed flatbread"),
    (2, "🧆", "falafel|chickpea|meatball"),
    (2, "🥚", "egg|food|breakfast"),
    (2, "🍳", "egg|pan|frying|cooking|breakfast"),
    (2, "🥘", "pan|food|paella|shallow|casserole|shallow pan of food"),
    (2, "🍲", "pot|stew|pot of food"),
    (2, "🫕", "pot|swiss|fondue|cheese|melted|chocolate"),
    (2, "🥣", "cereal|congee|breakfast|bowl with spoon"),
    (2, "🥗", "food|green|salad|green salad"),
    (2, "🍿", "popcorn"),
    (2, "🧈", "dairy|butter"),
    (2, "🧂", "salt|shaker|condiment"),
    (2, "🥫", "can|canned food"),
    (2, "🍱", "box|bento|bento box"),
    (2, "🍘", "rice|cracker|rice cracker"),
    (2, "🍙", "ball|rice|japanese|rice ball"),
    (2, "🍚", "rice|cooked|cooked rice"),
    (2, "🍛", "rice|curry|curry rice"),
    (2, "🍜", "bowl|ramen|noodle|steaming|steaming bowl"),
    (2, "🍝", "pasta|spaghetti"),
    (2, "🍠", "sweet|potato|roasted|roasted sweet potato"),
    (2, "🍢", "oden|kebab|stick|skewer|seafood"),
    (2, "🍣", "sushi"),
    (2, "🍤", "fried|prawn|shrimp|tempura|fried shrimp"),
    (2, "🍥", "cake|fish|swirl|pastry|fish cake with swirl"),
    (2, "🥮", "autumn|yuèbǐng|festival|moon cake"),
    (2, "🍡", "dango|stick|sweet|skewer|dessert|japanese"),
    (2, "🥟", "gyōza|jiaozi|pierogi|dumpling|empanada|potsticker"),
    (2, "🥠", "prophecy|fortune cookie"),
    (2, "🥡", "takeout box|oyster pail"),
    (2, "🦀", "crab|cancer|zodiac"),
    (2, "🦞", "claws|bisque|lobster|seafood"),
    (2, "🦐", "food|small|shrimp|shellfish"),
    (2, "🦑", "food|squid|molusc"),
    (2, "🦪", "pearl|oyster|diving"),
    (2, "🍦", "ice|soft|cream|sweet|dessert|icecream|soft ice cream"),
    (2, "🍧", "ice|sweet|shaved|dessert|shaved ice"),
    (2, "🍨", "ice|cream|sweet|dessert|ice cream"),
    (2, "🍩", "donut|sweet|dessert|doughnut|breakfast"),
    (2, "🍪", "sweet|cookie|dessert"),
    (2, "🎂", "cake|sweet|pastry|dessert|birthday|celebration|birthday cake"),
    (2, "🍰", "cake|slice|sweet|pastry|dessert|shortcake"),
    (2, "🧁", "sweet|bakery|cupcake"),
    (2, "🥧", "pie|pastry|filling"),
    (2, "🍫", "bar|sweet|dessert|chocolate|chocolate bar"),
    (2, "🍬", "candy|sweet|dessert"),
    (2, "🍭", "candy|sweet|dessert|lollipop"),
    (2, "🍮", "sweet|custard|dessert|pudding"),
    (2, "🍯", "pot|honey|sweet|honeypot|honey pot"),
    (2, "🍼", "baby|milk|drink|bottle|baby bottle"),
    (2, "🥛", "milk|drink|glass|glass of milk"),
    (2, "☕", "hot|tea|drink|coffee|beverage|steaming|hot beverage"),
    (2, "🫖", "pot|tea|drink|teapot"),
    (2, "🍵", "cup|tea|drink|teacup|beverage|teacup without handle"),
    (2, "🍶", "bar|cup|sake|drink|bottle|beverage"),
    (2, "🍾", "bar|cork|drink|bottle|popping|bottle with popping cork"),
    (2, "🍷", "bar|wine|drink|glass|beverage|wine glass"),
    (2, "🍸", "bar|drink|glass|cocktail|cocktail glass"),
    (2, "🍹", "bar|drink|tropical|tropical drink"),
    (2, "🍺", "bar|mug|beer|drink|beer mug"),
    (2, "🍻", "bar|mug|beer|clink|drink|clinking beer mugs"),
    (2, "🥂", "clink|drink|glass|celebrate|clinking glasses"),
    (2, "🥃", "shot|glass|liquor|whisky|tumbler|tumbler glass"),
    (2, "🫗", "drink|empty|glass|spill|pouring liquid"),
    (2, "🥤", "soda|juice|cup with straw"),
    (2, "🧋", "tea|milk|pearl|bubble|bubble tea"),
    (2, "🧃", "box|juice|straw|sweet|beverage|beverage box"),
    (2, "🧉", "mate|drink"),
    (2, "🧊", "ice|cold|iceberg|ice cube"),
    (2, "🥢", "hashi|chopsticks"),
    (2, "🍽️", "fork|knife|plate|cooking|fork and knife with plate"),
    (2, "🍴", "fork|knife|cooking|cutlery|fork and knife"),
    (2, "🥄", "spoon|tableware"),
    (2, "🔪", "tool|hocho|knife|weapon|cooking|kitchen knife"),
    (2, "🫙", "jar|empty|sauce|store|condiment|container"),
    (2, "🏺", "jug|drink|zodiac|amphora|cooking|aquarius"),
    (3, "🌍", "earth|globe|world|africa|europe|globe showing Europe Africa|globe showing europe africa"),
    (3, "🌎", "earth|globe|world|americas|globe showing Americas|globe showing americas"),
    (3, "🌏", "asia|earth|globe|world|australia|globe showing Asia Australia|globe showing asia australia"),
    (3, "🌐", "earth|globe|world|meridians|globe with meridians"),
    (3, "🗺️", "map|world|world map"),
    (3, "🗾", "map|japan|map of Japan|map of japan"),
    (3, "🧭", "compass|magnetic|navigation|orienteering"),
    (3, "🏔️", "cold|snow|mountain|snow capped mountain"),
    (3, "⛰️", "mountain"),
    (3, "🌋", "volcano|eruption|mountain"),
    (3, "🗻", "fuji|mountain|mount fuji"),
    (3, "🏕️", "camping"),
    (3, "🏖️", "beach|umbrella|beach with umbrella"),
    (3, "🏜️", "desert"),
    (3, "🏝️", "desert|island|desert island"),
    (3, "🏞️", "park|national park"),
    (3, "🏟️", "stadium"),
    (3, "🏛️", "classical|classical building"),
    (3, "🏗️", "construction|building construction"),
    (3, "🧱", "clay|wall|brick|bricks|mortar"),
    (3, "🪨", "rock|heavy|solid|stone|boulder"),
    (3, "🪵", "log|wood|lumber|timber"),
    (3, "🛖", "hut|yurt|house|roundhouse"),
    (3, "🏘️", "houses"),
    (3, "🏚️", "house|derelict|derelict house"),
    (3, "🏠", "home|house"),
    (3, "🏡", "home|house|garden|house with garden"),
    (3, "🏢", "building|office building"),
    (3, "🏣", "post|japanese|Japanese post office|japanese post office"),
    (3, "🏤", "post|european|post office"),
    (3, "🏥", "doctor|hospital|medicine"),
    (3, "🏦", "bank|building"),
    (3, "🏨", "hotel|building"),
    (3, "🏩", "love|hotel|love hotel"),
    (3, "🏪", "store|convenience|convenience store"),
    (3, "🏫", "school|building"),
    (3, "🏬", "store|department|department store"),
    (3, "🏭", "factory|building"),
    (3, "🏯", "castle|japanese|Japanese castle"),
    (3, "🏰", "castle|european"),
    (3, "💒", "chapel|wedding|romance"),
    (3, "🗼", "tokyo|tower|Tokyo tower"),
    (3, "🗽", "statue|liberty|Statue of Liberty|statue of liberty"),
    (3, "⛪", "cross|church|religion|christian"),
    (3, "🕌", "islam|mosque|muslim|religion"),
    (3, "🛕", "hindu|temple|hindu temple"),
    (3, "🕍", "jew|jewish|temple|religion|synagogue"),
    (3, "⛩️", "shinto|shrine|religion|shinto shrine"),
    (3, "🕋", "kaaba|islam|muslim|religion"),
    (3, "⛲", "fountain"),
    (3, "⛺", "tent|camping"),
    (3, "🌁", "fog|foggy"),
    (3, "🌃", "star|night|night with stars"),
    (3, "🏙️", "city|cityscape"),
    (3, "🌄", "sun|morning|sunrise|mountain|sunrise over mountains"),
    (3, "🌅", "sun|sunrise|morning"),
    (3, "🌆", "city|dusk|sunset|evening|landscape|cityscape at dusk"),
    (3, "🌇", "sun|dusk|sunset"),
    (3, "🌉", "night|bridge|bridge at night"),
    (3, "♨️", "hot|springs|steaming|hotsprings|hot springs"),
    (3, "🎠", "horse|carousel|carousel horse"),
    (3, "🛝", "play|theme park|amusement park|playground slide"),
    (3, "🎡", "wheel|ferris|theme park|ferris wheel|amusement park"),
    (3, "🎢", "roller|coaster|theme park|roller coaster|amusement park"),
    (3, "💈", "pole|barber|haircut|barber pole"),
    (3, "🎪", "tent|circus|circus tent"),
    (3, "🚂", "steam|train|engine|railway|locomotive"),
    (3, "🚃", "car|tram|train|railway|electric|trolleybus|railway car"),
    (3, "🚄", "speed|train|railway|shinkansen|high speed train"),
    (3, "🚅", "speed|train|bullet|railway|shinkansen|bullet train"),
    (3, "🚆", "train|railway"),
    (3, "🚇", "metro|subway"),
    (3, "🚈", "railway|light rail"),
    (3, "🚉", "train|station|railway"),
    (3, "🚊", "tram|trolleybus"),
    (3, "🚝", "vehicle|monorail"),
    (3, "🚞", "car|railway|mountain|mountain railway"),
    (3, "🚋", "car|tram|tram car|trolleybus"),
    (3, "🚌", "bus|vehicle"),
    (3, "🚍", "bus|oncoming|oncoming bus"),
    (3, "🚎", "bus|tram|trolley|trolleybus"),
    (3, "🚐", "bus|minibus"),
    (3, "🚑", "vehicle|ambulance"),
    (3, "🚒", "fire|truck|engine|fire engine"),
    (3, "🚓", "car|patrol|police|police car"),
    (3, "🚔", "car|police|oncoming|oncoming police car"),
    (3, "🚕", "taxi|vehicle"),
    (3, "🚖", "taxi|oncoming|oncoming taxi"),
    (3, "🚗", "car|automobile"),
    (3, "🚘", "car|oncoming|automobile|oncoming automobile"),
    (3, "🚙", "recreational|sport utility|sport utility vehicle"),
    (3, "🛻", "truck|pickup|pick up|pickup truck"),
    (3, "🚚", "truck|delivery|delivery truck"),
    (3, "🚛", "semi|lorry|truck|articulated lorry"),
    (3, "🚜", "tractor|vehicle"),
    (3, "🏎️", "car|racing|racing car"),
    (3, "🏍️", "racing|motorcycle"),
    (3, "🛵", "motor|scooter|motor scooter"),
    (3, "🦽", "accessibility|manual wheelchair"),
    (3, "🦼", "accessibility|motorized wheelchair"),
    (3, "🛺", "tuk tuk|auto rickshaw"),
    (3, "🚲", "bike|bicycle"),
    (3, "🛴", "kick|scooter|kick scooter"),
    (3, "🛹", "board|skateboard"),
    (3, "🛼", "skate|roller|roller skate"),
    (3, "🚏", "bus|stop|bus stop"),
    (3, "🛣️", "road|highway|motorway"),
    (3, "🛤️", "train|railway|railway track"),
    (3, "🛢️", "oil|drum|oil drum"),
    (3, "⛽", "gas|fuel|pump|diesel|station|fuelpump|fuel pump"),
    (3, "🛞", "tire|turn|wheel|circle"),
    (3, "🚨", "car|light|beacon|police|revolving|police car light"),
    (3, "🚥", "light|signal|traffic|horizontal traffic light"),
    (3, "🚦", "light|signal|traffic|vertical traffic light"),
    (3, "🛑", "sign|stop|stop sign|octagonal"),
    (3, "🚧", "barrier|construction"),
    (3, "⚓", "ship|tool|anchor"),
    (3, "🛟", "float|rescue|safety|ring buoy|life saver|life preserver"),
    (3, "⛵", "sea|boat|yacht|resort|sailboat"),
    (3, "🛶", "boat|canoe"),
    (3, "🚤", "boat|speedboat"),
    (3, "🛳️", "ship|passenger|passenger ship"),
    (3, "⛴️", "boat|ferry|passenger"),
    (3, "🛥️", "boat|motorboat|motor boat"),
    (3, "🚢", "ship|boat|passenger"),
    (3, "✈️", "airplane|aeroplane"),
    (3, "🛩️", "airplane|aeroplane|small airplane"),
    (3, "🛫", "airplane|check in|aeroplane|departure|departures|airplane departure"),
    (3, "🛬", "landing|airplane|arrivals|arriving|aeroplane|airplane arrival"),
    (3, "🪂", "skydive|parasail|parachute|hang glide"),
    (3, "💺", "seat|chair"),
    (3, "🚁", "vehicle|helicopter"),
    (3, "🚟", "railway|suspension|suspension railway"),
    (3, "🚠", "cable|gondola|mountain|mountain cableway"),
    (3, "🚡", "car|cable|aerial|gondola|tramway|aerial tramway"),
    (3, "🛰️", "space|satellite"),
    (3, "🚀", "space|rocket"),
    (3, "🛸", "ufo|flying saucer"),
    (3, "🛎️", "bell|hotel|bellhop|bellhop bell"),
    (3, "🧳", "travel|luggage|packing"),
    (3, "⌛", "sand|timer|hourglass done"),
    (3, "⏳", "sand|timer|hourglass|hourglass not done"),
    (3, "⌚", "watch|clock"),
    (3, "⏰", "alarm|clock|alarm clock"),
    (3, "⏱️", "clock|stopwatch"),
    (3, "⏲️", "clock|timer|timer clock"),
    (3, "🕰️", "clock|mantelpiece clock"),
    (3, "🕛", "00|12|12:00|clock|twelve|o’clock|twelve o’clock"),
    (3, "🕧", "12|12:30|clock|thirty|twelve|twelve thirty"),
    (3, "🕐", "1|00|one|1:00|clock|o’clock|one o’clock"),
    (3, "🕜", "1|one|1:30|clock|thirty|one thirty"),
    (3, "🕑", "2|00|two|2:00|clock|o’clock|two o’clock"),
    (3, "🕝", "2|two|2:30|clock|thirty|two thirty"),
    (3, "🕒", "3|00|3:00|clock|three|o’clock|three o’clock"),
    (3, "🕞", "3|3:30|clock|three|thirty|three thirty"),
    (3, "🕓", "4|00|4:00|four|clock|o’clock|four o’clock"),
    (3, "🕟", "4|4:30|four|clock|thirty|four thirty"),
    (3, "🕔", "5|00|5:00|five|clock|o’clock|five o’clock"),
    (3, "🕠", "5|5:30|five|clock|thirty|five thirty"),
    (3, "🕕", "6|00|six|6:00|clock|o’clock|six o’clock"),
    (3, "🕡", "6|six|6:30|clock|thirty|six thirty"),
    (3, "🕖", "7|00|7:00|clock|seven|o’clock|seven o’clock"),
    (3, "🕢", "7|7:30|clock|seven|thirty|seven thirty"),
    (3, "🕗", "8|00|8:00|clock|eight|o’clock|eight o’clock"),
    (3, "🕣", "8|8:30|clock|eight|thirty|eight thirty"),
    (3, "🕘", "9|00|9:00|nine|clock|o’clock|nine o’clock"),
    (3, "🕤", "9|9:30|nine|clock|thirty|nine thirty"),
    (3, "🕙", "00|10|ten|10:00|clock|o’clock|ten o’clock"),
    (3, "🕥", "10|ten|10:30|clock|thirty|ten thirty"),
    (3, "🕚", "00|11|11:00|clock|eleven|o’clock|eleven o’clock"),
    (3, "🕦", "11|11:30|clock|eleven|thirty|eleven thirty"),
    (3, "🌑", "dark|moon|new moon"),
    (3, "🌒", "moon|waxing|crescent|waxing crescent moon"),
    (3, "🌓", "moon|quarter|first quarter moon"),
    (3, "🌔", "moon|waxing|gibbous|waxing gibbous moon"),
    (3, "🌕", "full|moon|full moon"),
    (3, "🌖", "moon|waning|gibbous|waning gibbous moon"),
    (3, "🌗", "moon|quarter|last quarter moon"),
    (3, "🌘", "moon|waning|crescent|waning crescent moon"),
    (3, "🌙", "moon|crescent|crescent moon"),
    (3, "🌚", "face|moon|new moon face"),
    (3, "🌛", "face|moon|quarter|first quarter moon face"),
    (3, "🌜", "face|moon|quarter|last quarter moon face"),
    (3, "🌡️", "weather|thermometer"),
    (3, "☀️", "sun|rays|sunny|bright"),
    (3, "🌝", "face|full|moon|bright|full moon face"),
    (3, "🌞", "sun|face|bright|sun with face"),
    (3, "🪐", "saturn|saturnine|ringed planet"),
    (3, "⭐", "star"),
    (3, "🌟", "glow|star|shining|sparkle|glittery|glowing star"),
    (3, "🌠", "star|falling|shooting|shooting star"),
    (3, "🌌", "space|milky way"),
    (3, "☁️", "cloud|weather"),
    (3, "⛅", "sun|cloud|sun behind cloud"),
    (3, "⛈️", "rain|cloud|thunder|cloud with lightning and rain"),
    (3, "🌤️", "sun|cloud|sun behind small cloud"),
    (3, "🌥️", "sun|cloud|sun behind large cloud"),
    (3, "🌦️", "sun|rain|cloud|sun behind rain cloud"),
    (3, "🌧️", "rain|cloud|cloud with rain"),
    (3, "🌨️", "cold|snow|cloud|cloud with snow"),
    (3, "🌩️", "cloud|lightning|cloud with lightning"),
    (3, "🌪️", "cloud|tornado|whirlwind"),
    (3, "🌫️", "fog|cloud"),
    (3, "🌬️", "blow|face|wind|cloud|wind face"),
    (3, "🌀", "dizzy|cyclone|twister|typhoon|hurricane"),
    (3, "🌈", "rain|rainbow"),
    (3, "🌂", "rain|clothing|umbrella|closed umbrella"),
    (3, "☂️", "rain|umbrella|clothing"),
    (3, "☔", "drop|rain|clothing|umbrella|umbrella with rain drops"),
    (3, "⛱️", "sun|rain|umbrella|umbrella on ground"),
    (3, "⚡", "zap|danger|voltage|electric|lightning|high voltage"),
    (3, "❄️", "cold|snow|snowflake"),
    (3, "☃️", "cold|snow|snowman"),
    (3, "⛄", "cold|snow|snowman|snowman without snow"),
    (3, "☄️", "comet|space"),
    (3, "🔥", "fire|tool|flame"),
    (3, "💧", "cold|drop|comic|sweat|droplet"),
    (3, "🌊", "wave|ocean|water|water wave"),
    (4, "🎃", "jack|lantern|halloween|celebration|jack o lantern"),
    (4, "🎄", "tree|christmas|celebration|Christmas tree"),
    (4, "🎆", "fireworks|celebration"),
    (4, "🎇", "sparkle|sparkler|fireworks|celebration"),
    (4, "🧨", "dynamite|explosive|fireworks|firecracker"),
    (4, "✨", "*|star|sparkle|sparkles"),
    (4, "🎈", "balloon|celebration"),
    (4, "🎉", "tada|party|popper|celebration|party popper"),
    (4, "🎊", "ball|confetti|celebration|confetti ball"),
    (4, "🎋", "tree|banner|japanese|celebration|tanabata tree"),
    (4, "🎍", "pine|bamboo|japanese|celebration|pine decoration"),
    (4, "🎎", "doll|festival|japanese|celebration|Japanese dolls|japanese dolls"),
    (4, "🎏", "carp|streamer|celebration|carp streamer"),
    (4, "🎐", "bell|wind|chime|wind chime|celebration"),
    (4, "🎑", "moon|ceremony|celebration|moon viewing ceremony"),
    (4, "🧧", "gift|money|hóngbāo|lai see|good luck|red envelope"),
    (4, "🎀", "ribbon|celebration"),
    (4, "🎁", "box|gift|present|wrapped|celebration|wrapped gift"),
    (4, "🎗️", "ribbon|reminder|celebration|reminder ribbon"),
    (4, "🎟️", "ticket|admission|admission tickets"),
    (4, "🎫", "ticket|admission"),
    (4, "🎖️", "medal|military|celebration|military medal"),
    (4, "🏆", "prize|trophy"),
    (4, "🏅", "medal|sports medal"),
    (4, "🥇", "gold|first|medal|1st place medal"),
    (4, "🥈", "medal|second|silver|2nd place medal"),
    (4, "🥉", "medal|third|bronze|3rd place medal"),
    (4, "⚽", "ball|soccer|football|soccer ball"),
    (4, "⚾", "ball|baseball"),
    (4, "🥎", "ball|glove|softball|underarm"),
    (4, "🏀", "ball|hoop|basketball"),
    (4, "🏐", "ball|game|volleyball"),
    (4, "🏈", "ball|american|football|american football"),
    (4, "🏉", "ball|rugby|football|rugby football"),
    (4, "🎾", "ball|tennis|racquet"),
    (4, "🥏", "ultimate|flying disc"),
    (4, "🎳", "ball|game|bowling"),
    (4, "🏏", "bat|ball|game|cricket game"),
    (4, "🏑", "ball|game|field|stick|hockey|field hockey"),
    (4, "🏒", "ice|game|puck|stick|hockey|ice hockey"),
    (4, "🥍", "ball|goal|stick|lacrosse"),
    (4, "🏓", "bat|ball|game|paddle|ping pong|table tennis"),
    (4, "🏸", "game|birdie|racquet|badminton|shuttlecock"),
    (4, "🥊", "glove|boxing|boxing glove"),
    (4, "🥋", "judo|karate|uniform|taekwondo|martial arts|martial arts uniform"),
    (4, "🥅", "net|goal|goal net"),
    (4, "⛳", "golf|hole|flag in hole"),
    (4, "⛸️", "ice|skate|ice skate"),
    (4, "🎣", "fish|pole|fishing pole"),
    (4, "🤿", "scuba|diving|snorkeling|diving mask"),
    (4, "🎽", "sash|shirt|running|athletics|running shirt"),
    (4, "🎿", "ski|skis|snow"),
    (4, "🛷", "sled|sledge|sleigh"),
    (4, "🥌", "game|rock|curling stone"),
    (4, "🎯", "hit|dart|game|target|bullseye|direct hit"),
    (4, "🪀", "toy|yo yo|fluctuate"),
    (4, "🪁", "fly|kite|soar"),
    (4, "🔫", "gun|tool|water|pistol|weapon|handgun|revolver|water pistol"),
    (4, "🎱", "8|ball|game|eight|billiard|pool 8 ball"),
    (4, "🔮", "ball|tool|crystal|fantasy|fortune|fairy tale|crystal ball"),
    (4, "🪄", "magic|witch|wizard|magic wand"),
    (4, "🎮", "game|video game|controller"),
    (4, "🕹️", "game|joystick|video game"),
    (4, "🎰", "game|slot|slot machine"),
    (4, "🎲", "die|dice|game|game die"),
    (4, "🧩", "clue|piece|jigsaw|puzzle|puzzle piece|interlocking"),
    (4, "🧸", "toy|plush|stuffed|plaything|teddy bear"),
    (4, "🪅", "party|piñata|celebration"),
    (4, "🪩", "dance|disco|party|glitter|mirror ball"),
    (4, "🪆", "doll|russia|nesting|nesting dolls"),
    (4, "♠️", "card|game|spade suit"),
    (4, "♥️", "card|game|heart suit"),
    (4, "♦️", "card|game|diamond suit"),
    (4, "♣️", "card|game|club suit"),
    (4, "♟️", "dupe|chess|chess pawn|expendable"),
    (4, "🃏", "card|game|joker|wildcard"),
    (4, "🀄", "red|game|mahjong|mahjong red dragon"),
    (4, "🎴", "card|game|flower|playing|japanese|flower playing cards"),
    (4, "🎭", "art|mask|theater|theatre|performing|performing arts"),
    (4, "🖼️", "art|frame|museum|picture|painting|framed picture"),
    (4, "🎨", "art|museum|palette|painting|artist palette"),
    (4, "🧵", "spool|thread|needle|sewing|string"),
    (4, "🪡", "needle|sewing|sutures|stitches|tailoring|embroidery|sewing needle"),
    (4, "🧶", "yarn|ball|knit|crochet"),
    (4, "🪢", "tie|knot|rope|twine|twist|tangled"),
    (5, "👓", "eye|glasses|eyewear|clothing|eyeglasses"),
    (5, "🕶️", "eye|dark|eyewear|glasses|sunglasses"),
    (5, "🥽", "goggles|welding|swimming|eye protection"),
    (5, "🥼", "doctor|lab coat|scientist|experiment"),
    (5, "🦺", "vest|safety|emergency|safety vest"),
    (5, "👔", "tie|necktie|clothing"),
    (5, "👕", "shirt|tshirt|t shirt|clothing"),
    (5, "👖", "jeans|pants|clothing|trousers"),
    (5, "🧣", "neck|scarf"),
    (5, "🧤", "hand|gloves"),
    (5, "🧥", "coat|jacket"),
    (5, "🧦", "socks|stocking"),
    (5, "👗", "dress|clothing"),
    (5, "👘", "kimono|clothing"),
    (5, "🥻", "sari|dress|clothing"),
    (5, "🩱", "bathing suit|one piece swimsuit"),
    (5, "🩲", "briefs|swimsuit|one piece|underwear|bathing suit"),
    (5, "🩳", "pants|shorts|underwear|bathing suit"),
    (5, "👙", "swim|bikini|clothing"),
    (5, "👚", "woman|clothing|woman’s clothes"),
    (5, "🪭", "fan|hot|shy|dance|cooling|flutter|folding hand fan"),
    (5, "👛", "coin|purse|clothing"),
    (5, "👜", "bag|purse|handbag|clothing"),
    (5, "👝", "bag|pouch|clothing|clutch bag"),
    (5, "🛍️", "bag|hotel|shopping|shopping bags"),
    (5, "🎒", "bag|school|satchel|backpack|rucksack"),
    (5, "🩴", "zōri|thongs|sandals|thong sandal|beach sandals|thong sandals"),
    (5, "👞", "man|shoe|clothing|man’s shoe"),
    (5, "👟", "shoe|sneaker|athletic|clothing|running shoe"),
    (5, "🥾", "boot|hiking|camping|hiking boot|backpacking"),
    (5, "🥿", "slip on|slipper|flat shoe|ballet flat"),
    (5, "👠", "heel|shoe|woman|clothing|high heeled shoe"),
    (5, "👡", "shoe|woman|sandal|clothing|woman’s sandal"),
    (5, "🩰", "dance|ballet|ballet shoes"),
    (5, "👢", "boot|shoe|woman|clothing|woman’s boot"),
    (5, "🪮", "afro|comb|hair|pick|hair pick"),
    (5, "👑", "king|crown|queen|clothing"),
    (5, "👒", "hat|woman|clothing|woman’s hat"),
    (5, "🎩", "hat|top|tophat|top hat|clothing"),
    (5, "🎓", "cap|hat|clothing|graduation|celebration|graduation cap"),
    (5, "🧢", "billed cap|baseball cap"),
    (5, "🪖", "army|helmet|soldier|warrior|military|military helmet"),
    (5, "⛑️", "aid|hat|face|cross|helmet|rescue worker’s helmet"),
    (5, "📿", "beads|prayer|clothing|necklace|religion|prayer beads"),
    (5, "💄", "makeup|lipstick|cosmetics"),
    (5, "💍", "ring|diamond"),
    (5, "💎", "gem|jewel|diamond|gem stone"),
    (5, "🔇", "mute|quiet|silent|speaker|muted speaker"),
    (5, "🔈", "soft|speaker low volume"),
    (5, "🔉", "medium|speaker medium volume"),
    (5, "🔊", "loud|speaker high volume"),
    (5, "📢", "loud|loudspeaker|public address"),
    (5, "📣", "cheering|megaphone"),
    (5, "📯", "horn|post|postal|postal horn"),
    (5, "🔔", "bell"),
    (5, "🔕", "bell|mute|quiet|silent|forbidden|bell with slash"),
    (5, "🎼", "music|score|musical score"),
    (5, "🎵", "note|music|musical note"),
    (5, "🎶", "note|music|notes|musical notes"),
    (5, "🎙️", "mic|music|studio|microphone|studio microphone"),
    (5, "🎚️", "level|music|slider|level slider"),
    (5, "🎛️", "knobs|music|control|control knobs"),
    (5, "🎤", "mic|karaoke|microphone"),
    (5, "🎧", "earbud|headphone"),
    (5, "📻", "radio|video"),
    (5, "🎷", "sax|music|saxophone|instrument"),
    (5, "🪗", "accordion|concertina|squeeze box"),
    (5, "🎸", "music|guitar|instrument"),
    (5, "🎹", "music|piano|keyboard|instrument|musical keyboard"),
    (5, "🎺", "music|trumpet|instrument"),
    (5, "🎻", "music|violin|instrument"),
    (5, "🪕", "banjo|music|stringed"),
    (5, "🥁", "drum|music|drumsticks"),
    (5, "🪘", "beat|drum|conga|rhythm|long drum"),
    (5, "🪇", "music|shake|rattle|maracas|instrument|percussion"),
    (5, "🪈", "fife|pipe|flute|music|recorder|woodwind"),
    (5, "📱", "cell|phone|mobile|telephone|mobile phone"),
    (5, "📲", "cell|arrow|phone|mobile|receive|mobile phone with arrow"),
    (5, "☎️", "phone|telephone"),
    (5, "📞", "phone|receiver|telephone|telephone receiver"),
    (5, "📟", "pager"),
    (5, "📠", "fax|fax machine"),
    (5, "🔋", "battery"),
    (5, "🪫", "electronic|low energy|low battery"),
    (5, "🔌", "plug|electric|electricity|electric plug"),
    (5, "💻", "pc|laptop|computer|personal"),
    (5, "🖥️", "desktop|computer|desktop computer"),
    (5, "🖨️", "printer|computer"),
    (5, "⌨️", "keyboard|computer"),
    (5, "🖱️", "computer|computer mouse"),
    (5, "🖲️", "computer|trackball"),
    (5, "💽", "disk|optical|computer|minidisk|computer disk"),
    (5, "💾", "disk|floppy|computer|floppy disk"),
    (5, "💿", "cd|disk|optical|computer|optical disk"),
    (5, "📀", "dvd|disk|blu ray|optical|computer"),
    (5, "🧮", "abacus|calculation"),
    (5, "🎥", "movie|camera|cinema|movie camera"),
    (5, "🎞️", "film|movie|cinema|frames|film frames"),
    (5, "📽️", "film|movie|video|cinema|projector|film projector"),
    (5, "🎬", "movie|clapper|clapper board"),
    (5, "📺", "tv|video|television"),
    (5, "📷", "video|camera"),
    (5, "📸", "flash|video|camera|camera with flash"),
    (5, "📹", "video|camera|video camera"),
    (5, "📼", "vhs|tape|video|videocassette"),
    (5, "🔍", "tool|glass|search|magnifying|magnifying glass tilted left"),
    (5, "🔎", "tool|glass|search|magnifying|magnifying glass tilted right"),
    (5, "🕯️", "light|candle"),
    (5, "💡", "bulb|idea|comic|light|electric|light bulb"),
    (5, "🔦", "tool|light|torch|electric|flashlight"),
    (5, "🏮", "bar|red|light|lantern|red paper lantern"),
    (5, "🪔", "oil|diya|lamp|diya lamp"),
    (5, "📔", "book|cover|notebook|decorated|notebook with decorative cover"),
    (5, "📕", "book|closed|closed book"),
    (5, "📖", "book|open|open book"),
    (5, "📗", "book|green|green book"),
    (5, "📘", "blue|book|blue book"),
    (5, "📙", "book|orange|orange book"),
    (5, "📚", "book|books"),
    (5, "📓", "notebook"),
    (5, "📒", "ledger|notebook"),
    (5, "📃", "curl|page|document|page with curl"),
    (5, "📜", "paper|scroll"),
    (5, "📄", "page|document|page facing up"),
    (5, "📰", "news|paper|newspaper"),
    (5, "🗞️", "news|paper|rolled|newspaper|rolled up newspaper"),
    (5, "📑", "mark|tabs|marker|bookmark|bookmark tabs"),
    (5, "🔖", "mark|bookmark"),
    (5, "🏷️", "label"),
    (5, "💰", "bag|money|dollar|moneybag|money bag"),
    (5, "🪙", "coin|gold|metal|money|silver|treasure"),
    (5, "💴", "yen|bill|note|money|banknote|currency|yen banknote"),
    (5, "💵", "bill|note|money|dollar|banknote|currency|dollar banknote"),
    (5, "💶", "bill|euro|note|money|banknote|currency|euro banknote"),
    (5, "💷", "bill|note|money|pound|banknote|currency|pound banknote"),
    (5, "💸", "fly|bill|money|wings|banknote|money with wings"),
    (5, "💳", "card|money|credit|credit card"),
    (5, "🧾", "proof|receipt|evidence|accounting|bookkeeping"),
    (5, "💹", "yen|chart|graph|money|growth|chart increasing with yen"),
    (5, "✉️", "email|letter|envelope"),
    (5, "📧", "mail|email|e mail|letter"),
    (5, "📨", "email|e mail|letter|receive|envelope|incoming|incoming envelope"),
    (5, "📩", "arrow|email|e mail|envelope|outgoing|envelope with arrow"),
    (5, "📤", "box|mail|sent|tray|letter|outbox|outbox tray"),
    (5, "📥", "box|mail|tray|inbox|letter|receive|inbox tray"),
    (5, "📦", "box|parcel|package"),
    (5, "📫", "mail|closed|mailbox|postbox|closed mailbox with raised flag"),
    (5, "📪", "mail|closed|lowered|mailbox|postbox|closed mailbox with lowered flag"),
    (5, "📬", "mail|open|mailbox|postbox|open mailbox with raised flag"),
    (5, "📭", "mail|open|lowered|mailbox|postbox|open mailbox with lowered flag"),
    (5, "📮", "mail|postbox|mailbox"),
    (5, "🗳️", "box|ballot|ballot box with ballot"),
    (5, "✏️", "pencil"),
    (5, "✒️", "nib|pen|black nib"),
    (5, "🖋️", "pen|fountain|fountain pen"),
    (5, "🖊️", "pen|ballpoint"),
    (5, "🖌️", "painting|paintbrush"),
    (5, "🖍️", "crayon"),
    (5, "📝", "memo|pencil"),
    (5, "💼", "briefcase"),
    (5, "📁", "file|folder|file folder"),
    (5, "📂", "file|open|folder|open file folder"),
    (5, "🗂️", "card|index|dividers|card index dividers"),
    (5, "📅", "date|calendar"),
    (5, "📆", "calendar|tear off calendar"),
    (5, "🗒️", "pad|note|spiral|spiral notepad"),
    (5, "🗓️", "pad|spiral|calendar|spiral calendar"),
    (5, "📇", "card|index|rolodex|card index"),
    (5, "📈", "chart|graph|trend|growth|upward|chart increasing"),
    (5, "📉", "down|chart|graph|trend|chart decreasing"),
    (5, "📊", "bar|chart|graph|bar chart"),
    (5, "📋", "clipboard"),
    (5, "📌", "pin|pushpin"),
    (5, "📍", "pin|pushpin|round pushpin"),
    (5, "📎", "paperclip"),
    (5, "🖇️", "link|paperclip|linked paperclips"),
    (5, "📏", "ruler|straight edge|straight ruler"),
    (5, "📐", "set|ruler|triangle|triangular ruler"),
    (5, "✂️", "tool|cutting|scissors"),
    (5, "🗃️", "box|card|file|card file box"),
    (5, "🗄️", "file|filing|cabinet|file cabinet"),
    (5, "🗑️", "wastebasket"),
    (5, "🔒", "locked|closed"),
    (5, "🔓", "lock|open|unlock|unlocked"),
    (5, "🔏", "ink|nib|pen|lock|privacy|locked with pen"),
    (5, "🔐", "key|lock|closed|secure|locked with key"),
    (5, "🔑", "key|lock|password"),
    (5, "🗝️", "key|old|clue|lock|old key"),
    (5, "🔨", "tool|hammer"),
    (5, "🪓", "axe|chop|wood|split|hatchet"),
    (5, "⛏️", "pick|tool|mining"),
    (5, "⚒️", "pick|tool|hammer|hammer and pick"),
    (5, "🛠️", "tool|hammer|wrench|spanner|hammer and wrench"),
    (5, "🗡️", "knife|dagger|weapon"),
    (5, "⚔️", "swords|weapon|crossed|crossed swords"),
    (5, "💣", "bomb|comic"),
    (5, "🪃", "rebound|boomerang|repercussion"),
    (5, "🏹", "bow|arrow|archer|zodiac|sagittarius|bow and arrow"),
    (5, "🛡️", "shield|weapon"),
    (5, "🪚", "saw|tool|lumber|carpenter|carpentry saw"),
    (5, "🔧", "tool|wrench|spanner"),
    (5, "🪛", "tool|screw|screwdriver"),
    (5, "🔩", "nut|bolt|tool|nut and bolt"),
    (5, "⚙️", "cog|gear|tool|cogwheel"),
    (5, "🗜️", "tool|vice|clamp|compress"),
    (5, "⚖️", "libra|scale|zodiac|balance|justice|balance scale"),
    (5, "🦯", "blind|white cane|accessibility"),
    (5, "🔗", "link"),
    (5, "⛓️‍💥", "break|chain|cuffs|freedom|breaking|broken chain"),
    (5, "⛓️", "chain|chains"),
    (5, "🪝", "hook|catch|crook|curve|ensnare|selling point"),
    (5, "🧰", "tool|chest|toolbox|mechanic"),
    (5, "🧲", "magnet|magnetic|horseshoe|attraction"),
    (5, "🪜", "rung|step|climb|ladder"),
    (5, "⚗️", "tool|alembic|chemistry"),
    (5, "🧪", "lab|chemist|science|test tube|chemistry|experiment"),
    (5, "🧫", "lab|biology|culture|bacteria|biologist|petri dish"),
    (5, "🧬", "dna|gene|life|genetics|biologist|evolution"),
    (5, "🔬", "tool|science|microscope"),
    (5, "🔭", "tool|science|telescope"),
    (5, "📡", "dish|antenna|satellite|satellite antenna"),
    (5, "💉", "shot|sick|needle|syringe|medicine"),
    (5, "🩸", "bleed|injury|medicine|menstruation|drop of blood|blood donation"),
    (5, "💊", "pill|sick|doctor|medicine"),
    (5, "🩹", "bandage|adhesive bandage"),
    (5, "🩼", "cane|hurt|stick|crutch|disability|mobility aid"),
    (5, "🩺", "heart|doctor|medicine|stethoscope"),
    (5, "🩻", "x ray|bones|doctor|medical|skeleton"),
    (5, "🚪", "door"),
    (5, "🛗", "lift|hoist|elevator|accessibility"),
    (5, "🪞", "mirror|speculum|reflector|reflection"),
    (5, "🪟", "view|frame|window|opening|fresh air|transparent"),
    (5, "🛏️", "bed|hotel|sleep"),
    (5, "🛋️", "lamp|couch|hotel|couch and lamp"),
    (5, "🪑", "sit|seat|chair"),
    (5, "🚽", "toilet"),
    (5, "🪠", "toilet|plunger|plumber|suction|force cup"),
    (5, "🚿", "water|shower"),
    (5, "🛁", "bath|bathtub"),
    (5, "🪤", "bait|trap|snare|mousetrap|mouse trap"),
    (5, "🪒", "razor|sharp|shave"),
    (5, "🧴", "lotion|shampoo|sunscreen|moisturizer|lotion bottle"),
    (5, "🧷", "diaper|punk rock|safety pin"),
    (5, "🧹", "broom|witch|cleaning|sweeping"),
    (5, "🧺", "basket|picnic|farming|laundry"),
    (5, "🧻", "paper towels|toilet paper|roll of paper"),
    (5, "🪣", "vat|cask|pail|bucket"),
    (5, "🧼", "bar|soap|lather|bathing|cleaning|soapdish"),
    (5, "🫧", "burp|soap|clean|bubbles|underwater"),
    (5, "🪥", "brush|clean|teeth|dental|hygiene|bathroom|toothbrush"),
    (5, "🧽", "sponge|porous|cleaning|absorbing"),
    (5, "🧯", "fire|quench|extinguish|fire extinguisher"),
    (5, "🛒", "cart|trolley|shopping|shopping cart"),
    (5, "🚬", "smoking|cigarette"),
    (5, "⚰️", "death|coffin"),
    (5, "🪦", "grave|cemetery|headstone|graveyard|tombstone"),
    (5, "⚱️", "urn|ashes|death|funeral|funeral urn"),
    (5, "🧿", "bead|charm|nazar|evil eye|talisman|nazar amulet"),
    (5, "🪬", "hand|mary|hamsa|amulet|fatima|miriam|protection"),
    (5, "🗿", "moai|face|moyai|statue"),
    (5, "🪧", "sign|picket|placard|protest|demonstration"),
    (5, "🪪", "id|license|security|credentials|identification card"),
    (6, "🏧", "atm|bank|teller|ATM sign|atm sign|automated"),
    (6, "🚮", "litter|litter bin|litter in bin sign"),
    (6, "🚰", "water|potable|drinking|potable water"),
    (6, "♿", "access|wheelchair symbol"),
    (6, "🚹", "wc|man|toilet|bathroom|lavatory|restroom|men’s room"),
    (6, "🚺", "wc|woman|toilet|bathroom|lavatory|restroom|women’s room"),
    (6, "🚻", "wc|toilet|restroom|bathroom|lavatory"),
    (6, "🚼", "baby|changing|baby symbol"),
    (6, "🚾", "wc|water|closet|toilet|bathroom|lavatory|restroom|water closet"),
    (6, "🛂", "control|passport|passport control"),
    (6, "🛃", "customs"),
    (6, "🛄", "claim|baggage|baggage claim"),
    (6, "🛅", "locker|baggage|luggage|left luggage"),
    (6, "⚠️", "warning"),
    (6, "🚸", "child|traffic|crossing|pedestrian|children crossing"),
    (6, "⛔", "no|not|entry|traffic|no entry|forbidden|prohibited"),
    (6, "🚫", "no|not|entry|forbidden|prohibited"),
    (6, "🚳", "no|bike|bicycle|forbidden|prohibited|no bicycles"),
    (6, "🚭", "no|not|smoking|forbidden|no smoking|prohibited"),
    (6, "🚯", "no|not|litter|forbidden|prohibited|no littering"),
    (6, "🚱", "water|non potable|non drinking|non potable water"),
    (6, "🚷", "no|not|forbidden|pedestrian|prohibited|no pedestrians"),
    (6, "📵", "no|cell|phone|mobile|forbidden|no mobile phones"),
    (6, "🔞", "18|eighteen|underage|prohibited|age restriction|no one under eighteen"),
    (6, "☢️", "sign|radioactive"),
    (6, "☣️", "sign|biohazard"),
    (6, "⬆️", "arrow|north|up arrow|cardinal|direction"),
    (6, "↗️", "arrow|direction|northeast|intercardinal|up right arrow"),
    (6, "➡️", "east|arrow|cardinal|direction|right arrow"),
    (6, "↘️", "arrow|direction|southeast|intercardinal|down right arrow"),
    (6, "⬇️", "down|arrow|south|cardinal|direction|down arrow"),
    (6, "↙️", "arrow|direction|southwest|intercardinal|down left arrow"),
    (6, "⬅️", "west|arrow|cardinal|direction|left arrow"),
    (6, "↖️", "arrow|direction|northwest|up left arrow|intercardinal"),
    (6, "↕️", "arrow|up down arrow"),
    (6, "↔️", "arrow|left right arrow"),
    (6, "↩️", "arrow|right arrow curving left"),
    (6, "↪️", "arrow|left arrow curving right"),
    (6, "⤴️", "arrow|right arrow curving up"),
    (6, "⤵️", "down|arrow|right arrow curving down"),
    (6, "🔃", "arrow|reload|clockwise|clockwise vertical arrows"),
    (6, "🔄", "arrow|withershins|anticlockwise|counterclockwise|counterclockwise arrows button"),
    (6, "🔙", "back|arrow|BACK arrow"),
    (6, "🔚", "end|arrow|END arrow"),
    (6, "🔛", "on|on!|mark|arrow|ON! arrow"),
    (6, "🔜", "soon|arrow|SOON arrow"),
    (6, "🔝", "up|top|arrow|TOP arrow"),
    (6, "🛐", "worship|religion|place of worship"),
    (6, "⚛️", "atom|atheist|atom symbol"),
    (6, "🕉️", "om|hindu|religion"),
    (6, "✡️", "jew|star|david|jewish|religion|star of David|star of david"),
    (6, "☸️", "wheel|dharma|buddhist|religion|wheel of dharma"),
    (6, "☯️", "tao|yin|yang|taoist|yin yang|religion"),
    (6, "✝️", "cross|religion|christian|latin cross"),
    (6, "☦️", "cross|religion|christian|orthodox cross"),
    (6, "☪️", "islam|muslim|religion|star and crescent"),
    (6, "☮️", "peace|peace symbol"),
    (6, "🕎", "menorah|religion|candelabrum|candlestick"),
    (6, "🔯", "star|fortune|dotted six pointed star"),
    (6, "🪯", "sikh|khanda|religion"),
    (6, "♈", "ram|Aries|aries|zodiac"),
    (6, "♉", "ox|bull|Taurus|taurus|zodiac"),
    (6, "♊", "twins|Gemini|gemini|zodiac"),
    (6, "♋", "crab|Cancer|cancer|zodiac"),
    (6, "♌", "Leo|leo|lion|zodiac"),
    (6, "♍", "Virgo|virgo|zodiac"),
    (6, "♎", "Libra|libra|scales|zodiac|balance|justice"),
    (6, "♏", "zodiac|Scorpio|scorpio|scorpion|scorpius"),
    (6, "♐", "archer|zodiac|Sagittarius|sagittarius"),
    (6, "♑", "goat|zodiac|Capricorn|capricorn"),
    (6, "♒", "water|bearer|zodiac|Aquarius|aquarius"),
    (6, "♓", "fish|Pisces|pisces|zodiac"),
    (6, "⛎", "snake|bearer|zodiac|serpent|Ophiuchus|ophiuchus"),
    (6, "🔀", "arrow|crossed|shuffle tracks button"),
    (6, "🔁", "arrow|repeat|clockwise|repeat button"),
    (6, "🔂", "once|arrow|clockwise|repeat single button"),
    (6, "▶️", "play|arrow|right|triangle|play button"),
    (6, "⏩", "fast|arrow|double|forward|fast forward button"),
    (6, "⏭️", "arrow|triangle|next scene|next track|next track button"),
    (6, "⏯️", "play|arrow|pause|right|triangle|play or pause button"),
    (6, "◀️", "left|arrow|reverse|triangle|reverse button"),
    (6, "⏪", "arrow|double|rewind|fast reverse button"),
    (6, "⏮️", "arrow|triangle|previous scene|previous track|last track button"),
    (6, "🔼", "arrow|button|upwards button"),
    (6, "⏫", "arrow|double|fast up button"),
    (6, "🔽", "down|arrow|button|downwards button"),
    (6, "⏬", "down|arrow|double|fast down button"),
    (6, "⏸️", "bar|pause|double|vertical|pause button"),
    (6, "⏹️", "stop|square|stop button"),
    (6, "⏺️", "circle|record|record button"),
    (6, "⏏️", "eject|eject button"),
    (6, "🎦", "film|movie|cinema|camera"),
    (6, "🔅", "dim|low|dim button|brightness"),
    (6, "🔆", "bright|brightness|bright button"),
    (6, "📶", "bar|cell|phone|mobile|antenna|antenna bars"),
    (6, "🛜", "wifi|wi fi|network|wireless|computer|internet"),
    (6, "📳", "cell|mode|phone|mobile|telephone|vibration|vibration mode"),
    (6, "📴", "off|cell|phone|mobile|telephone|mobile phone off"),
    (6, "♀️", "woman|female sign"),
    (6, "♂️", "man|male sign"),
    (6, "⚧️", "transgender|transgender symbol"),
    (6, "✖️", "x|×|sign|cancel|multiply|multiplication"),
    (6, "➕", "+|plus|math|sign"),
    (6, "➖", " |−|math|sign|minus"),
    (6, "➗", "÷|math|sign|divide|division"),
    (6, "🟰", "math|equality|heavy equals sign"),
    (6, "♾️", "forever|infinity|unbounded|universal"),
    (6, "‼️", "!|!!|mark|bangbang|exclamation|double exclamation mark"),
    (6, "⁉️", "!|?|!?|mark|question|exclamation|interrobang|punctuation|exclamation question mark"),
    (6, "❓", "?|mark|question|punctuation|red question mark"),
    (6, "❔", "?|mark|outlined|question|punctuation|white question mark"),
    (6, "❕", "!|mark|outlined|exclamation|punctuation|white exclamation mark"),
    (6, "❗", "!|mark|exclamation|punctuation|red exclamation mark"),
    (6, "〰️", "dash|wavy|wavy dash|punctuation"),
    (6, "💱", "bank|money|currency|exchange|currency exchange"),
    (6, "💲", "money|dollar|currency|heavy dollar sign"),
    (6, "⚕️", "staff|medicine|aesculapius|medical symbol"),
    (6, "♻️", "recycle|recycling symbol"),
    (6, "⚜️", "fleur de lis"),
    (6, "🔱", "ship|tool|anchor|emblem|trident|trident emblem"),
    (6, "📛", "name|badge|name badge"),
    (6, "🔰", "leaf|chevron|beginner|japanese|Japanese symbol for beginner|japanese symbol for beginner"),
    (6, "⭕", "o|red|large|circle|hollow red circle"),
    (6, "✅", "✓|mark|check|button|check mark button"),
    (6, "☑️", "✓|box|check|check box with check"),
    (6, "✔️", "✓|mark|check|check mark"),
    (6, "❌", "x|×|mark|cross|cancel|multiply|cross mark|multiplication"),
    (6, "❎", "x|×|mark|square|cross mark button"),
    (6, "➰", "curl|loop|curly loop"),
    (6, "➿", "curl|loop|double|double curly loop"),
    (6, "〽️", "mark|part|part alternation mark"),
    (6, "✳️", "*|asterisk|eight spoked asterisk"),
    (6, "✴️", "*|star|eight pointed star"),
    (6, "❇️", "*|sparkle"),
    (6, "©️", "c|copyright"),
    (6, "®️", "r|registered"),
    (6, "™️", "tm|mark|trademark|trade mark"),
    (6, "#️⃣", "keycap|keycap: #"),
    (6, "*️⃣", "keycap|keycap: *"),
    (6, "0️⃣", "keycap|keycap: 0"),
    (6, "1️⃣", "keycap|keycap: 1"),
    (6, "2️⃣", "keycap|keycap: 2"),
    (6, "3️⃣", "keycap|keycap: 3"),
    (6, "4️⃣", "keycap|keycap: 4"),
    (6, "5️⃣", "keycap|keycap: 5"),
    (6, "6️⃣", "keycap|keycap: 6"),
    (6, "7️⃣", "keycap|keycap: 7"),
    (6, "8️⃣", "keycap|keycap: 8"),
    (6, "9️⃣", "keycap|keycap: 9"),
    (6, "🔟", "keycap|keycap: 10"),
    (6, "🔠", "abcd|input|latin|letters|uppercase|input latin uppercase"),
    (6, "🔡", "abcd|input|latin|letters|lowercase|input latin lowercase"),
    (6, "🔢", "1234|input|numbers|input numbers"),
    (6, "🔣", "〒♪&%|input|input symbols"),
    (6, "🔤", "abc|input|latin|letters|alphabet|input latin letters"),
    (6, "🅰️", "a|blood type|A button (blood type)|a button (blood type)"),
    (6, "🆎", "ab|blood type|AB button (blood type)|ab button (blood type)"),
    (6, "🅱️", "b|blood type|B button (blood type)|b button (blood type)"),
    (6, "🆑", "cl|CL button|cl button"),
    (6, "🆒", "cool|COOL button|cool button"),
    (6, "🆓", "free|FREE button|free button"),
    (6, "ℹ️", "i|information"),
    (6, "🆔", "id|identity|ID button|id button"),
    (6, "Ⓜ️", "m|circle|circled M|circled m"),
    (6, "🆕", "new|NEW button|new button"),
    (6, "🆖", "ng|NG button|ng button"),
    (6, "🅾️", "o|blood type|O button (blood type)|o button (blood type)"),
    (6, "🆗", "ok|OK button|ok button"),
    (6, "🅿️", "p|parking|P button|p button"),
    (6, "🆘", "sos|help|SOS button|sos button"),
    (6, "🆙", "up|up!|mark|UP! button|up! button"),
    (6, "🆚", "vs|versus|VS button|vs button"),
    (6, "🈁", "ココ|“here”|japanese|katakana|Japanese “here” button|japanese “here” button"),
    (6, "🈂️", "サ|japanese|katakana|“service charge”|Japanese “service charge” button|japanese “service charge” button"),
    (6, "🈷️", "月|japanese|ideograph|“monthly amount”|Japanese “monthly amount” button|japanese “monthly amount” button"),
    (6, "🈶", "有|japanese|ideograph|“not free of charge”|Japanese “not free of charge” button|japanese “not free of charge” button"),
    (6, "🈯", "指|japanese|ideograph|“reserved”|Japanese “reserved” button|japanese “reserved” button"),
    (6, "🉐", "得|japanese|ideograph|“bargain”|Japanese “bargain” button|japanese “bargain” button"),
    (6, "🈹", "割|japanese|ideograph|“discount”|Japanese “discount” button|japanese “discount” button"),
    (6, "🈚", "無|japanese|ideograph|“free of charge”|Japanese “free of charge” button|japanese “free of charge” button"),
    (6, "🈲", "禁|japanese|ideograph|“prohibited”|Japanese “prohibited” button|japanese “prohibited” button"),
    (6, "🉑", "可|japanese|ideograph|“acceptable”|Japanese “acceptable” button|japanese “acceptable” button"),
    (6, "🈸", "申|japanese|ideograph|“application”|Japanese “application” button|japanese “application” button"),
    (6, "🈴", "合|japanese|ideograph|“passing grade”|Japanese “passing grade” button|japanese “passing grade” button"),
    (6, "🈳", "空|japanese|ideograph|“vacancy”|Japanese “vacancy” button|japanese “vacancy” button"),
    (6, "㊗️", "祝|japanese|ideograph|“congratulations”|Japanese “congratulations” button|japanese “congratulations” button"),
    (6, "㊙️", "秘|japanese|“secret”|ideograph|Japanese “secret” button|japanese “secret” button"),
    (6, "🈺", "営|japanese|ideograph|“open for business”|Japanese “open for business” button|japanese “open for business” button"),
    (6, "🈵", "満|japanese|ideograph|“no vacancy”|Japanese “no vacancy” button|japanese “no vacancy” button"),
    (6, "🔴", "red|circle|geometric|red circle"),
    (6, "🟠", "circle|orange|orange circle"),
    (6, "🟡", "circle|yellow|yellow circle"),
    (6, "🟢", "green|circle|green circle"),
    (6, "🔵", "blue|circle|geometric|blue circle"),
    (6, "🟣", "circle|purple|purple circle"),
    (6, "🟤", "brown|circle|brown circle"),
    (6, "⚫", "circle|geometric|black circle"),
    (6, "⚪", "circle|geometric|white circle"),
    (6, "🟥", "red|square|red square"),
    (6, "🟧", "orange|square|orange square"),
    (6, "🟨", "square|yellow|yellow square"),
    (6, "🟩", "green|square|green square"),
    (6, "🟦", "blue|square|blue square"),
    (6, "🟪", "purple|square|purple square"),
    (6, "🟫", "brown|square|brown square"),
    (6, "⬛", "square|geometric|black large square"),
    (6, "⬜", "square|geometric|white large square"),
    (6, "◼️", "square|geometric|black medium square"),
    (6, "◻️", "square|geometric|white medium square"),
    (6, "◾", "square|geometric|black medium small square"),
    (6, "◽", "square|geometric|white medium small square"),
    (6, "▪️", "square|geometric|black small square"),
    (6, "▫️", "square|geometric|white small square"),
    (6, "🔶", "orange|diamond|geometric|large orange diamond"),
    (6, "🔷", "blue|diamond|geometric|large blue diamond"),
    (6, "🔸", "orange|diamond|geometric|small orange diamond"),
    (6, "🔹", "blue|diamond|geometric|small blue diamond"),
    (6, "🔺", "red|geometric|red triangle pointed up"),
    (6, "🔻", "red|down|geometric|red triangle pointed down"),
    (6, "💠", "comic|inside|diamond|geometric|diamond with a dot"),
    (6, "🔘", "radio|button|geometric|radio button"),
    (6, "🔳", "button|square|outlined|geometric|white square button"),
    (6, "🔲", "button|square|geometric|black square button"),
];

/// Kategori düğmeleri hareketli grafiği (200×80, 10 sütun × 4 satır; satır 1 etkin, 2 koyu tema).
pub const NAV_SPRITE: &str = r##"<svg version="1.1" id="Layer_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px"
	 width="200px" height="80px" viewBox="0 0 200 80" enable-background="new 0 0 200 80" xml:space="preserve">
<g id="Layer_11">
	<g>
		<path fill="#3371B7" stroke="#3371B7" stroke-width="0.1" stroke-miterlimit="10" d="M12.8,29.5c0.6,0,1.1-0.5,1.1-1.1
			c0-0.6-0.5-1.2-1.1-1.2c-0.6,0-1.2,0.5-1.2,1.2C11.6,29,12.2,29.5,12.8,29.5z M12.8,28c0.2,0,0.4,0.2,0.4,0.4
			c0,0.2-0.2,0.4-0.4,0.4c-0.2,0-0.4-0.2-0.4-0.4C12.4,28.1,12.6,28,12.8,28z"/>
		<path fill="#3371B7" stroke="#3371B7" stroke-width="0.1" stroke-miterlimit="10" d="M10,23c-3.8,0-7,3.1-7,7c0,3.8,3.1,7,7,7
			s7-3.1,7-7C17,26.2,13.8,23,10,23z M10,36.2c-3.4,0-6.2-2.8-6.2-6.2c0-3.4,2.8-6.2,6.2-6.2s6.2,2.8,6.2,6.2
			C16.2,33.4,13.4,36.2,10,36.2z"/>
		<path fill="#3371B7" stroke="#3371B7" stroke-width="0.1" stroke-miterlimit="10" d="M14.6,31.2c-0.1-0.1-0.2-0.2-0.3-0.2H5.7
			c-0.1,0-0.2,0.1-0.3,0.2c-0.1,0.1-0.1,0.2,0,0.4c0.7,2,2.5,3.3,4.6,3.3s3.9-1.3,4.6-3.3C14.7,31.5,14.7,31.3,14.6,31.2z M10,34.1
			c-1.6,0-3-0.9-3.7-2.2h7.3C13,33.2,11.6,34.1,10,34.1z"/>
		<path fill="#3371B7" stroke="#3371B7" stroke-width="0.1" stroke-miterlimit="10" d="M7.2,29.5c0.6,0,1.2-0.5,1.2-1.1
			c0-0.6-0.5-1.2-1.2-1.2c-0.6,0-1.1,0.5-1.1,1.2C6.1,29,6.6,29.5,7.2,29.5z M7.2,28c0.2,0,0.4,0.2,0.4,0.4c0,0.2-0.2,0.4-0.4,0.4
			c-0.2,0-0.4-0.2-0.4-0.4C6.8,28.1,7,28,7.2,28z"/>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#3371B7" d="M64.1,33.4l2.3,0c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.3
				c-0.2,0-0.4-0.2-0.4-0.4v-2.1C63.7,33.6,63.8,33.4,64.1,33.4z"/>
			<path fill="none" stroke="#3371B7" d="M73.5,33.5h2.4c0.2,0,0.4,0.2,0.4,0.4v2c0,0.2-0.2,0.4-0.4,0.4h-2.4
				c-0.2,0-0.4-0.2-0.4-0.4l0-2C73.1,33.6,73.3,33.5,73.5,33.5z"/>
			<path fill="none" stroke="#3371B7" d="M63.7,28.4h12.6v5H63.7V28.4z"/>
			<path fill="none" stroke="#3371B7" d="M65.5,23.6h8.9c1,0,1.9,0.8,1.9,1.9v3.1H63.7v-3.1C63.7,24.5,64.5,23.6,65.5,23.6z"/>
			<ellipse fill="#3371B7" cx="66.2" cy="30.9" rx="0.9" ry="1"/>
			<ellipse fill="#3371B7" cx="73.8" cy="30.9" rx="0.9" ry="1"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#3371B7" d="M96.4,30c0,3.6-2.9,6.5-6.4,6.5s-6.4-2.9-6.4-6.5s2.9-6.5,6.4-6.5S96.4,26.4,96.4,30z"/>
			<path fill="none" stroke="#3371B7" d="M96.3,28.6c0,0,0,0.1,0,0.1c-0.9,0.1-2.9,0.1-4.6-1.1c-1.1-0.8-2-1.7-2.6-2.5
				c-0.3-0.4-0.6-0.8-0.7-1c-0.1-0.1-0.1-0.1-0.1-0.2c0.5-0.1,1.2-0.2,2-0.1c1.2,0,2.5,0.3,3.5,1.1c1,0.8,1.7,1.8,2.1,2.8
				C96.1,27.9,96.2,28.3,96.3,28.6z"/>
			<path fill="none" stroke="#3371B7" d="M84,32.2c0,0,0-0.1,0-0.1c0.9-0.2,2.9-0.4,4.7,0.6c1.1,0.7,1.9,1.5,2.4,2.3
				c0.4,0.5,0.6,1,0.7,1.3c-0.4,0.1-1,0.2-1.7,0.3c-1,0-2.1-0.1-3.2-0.8s-1.9-1.6-2.4-2.5C84.2,32.8,84.1,32.5,84,32.2z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M116.3,26.8l-1.4,2l-0.8-0.8l-0.6-0.6l0,0.9l-0.1,8.2h-6.8
				l-0.1-8.2l0-0.9l-0.6,0.6l-0.8,0.8l-1.4-2l2.6-2.9c0.1-0.1,0.2-0.1,0.3-0.1h1.3l0.4,0.7c0.7,1.3,2.6,1.3,3.3-0.1l0.3-0.6h1.2
				c0.1,0,0.2,0,0.3,0.1l0.3-0.3l-0.3,0.3L116.3,26.8z"/>
			<path fill="none" stroke="#3371B7" d="M110.1,27.7h2v0.9c0,0.4-0.4,0.7-1,0.7c-0.6,0-1-0.3-1-0.7L110.1,27.7L110.1,27.7z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#3371B7" d="M126.8,34.3c0,1.2-1,2.2-2.2,2.2s-2.2-1-2.2-2.2s1-2.2,2.2-2.2S126.8,33.1,126.8,34.3z"/>
			<path fill="none" stroke="#3371B7" d="M137.6,34.3c0,1.2-1,2.2-2.2,2.2c-1.2,0-2.2-1-2.2-2.2s1-2.2,2.2-2.2
				C136.6,32.1,137.6,33.1,137.6,34.3z"/>
			<path fill="none" stroke="#3371B7" d="M126.8,24.4v9.9"/>
			<path fill="none" stroke="#3371B7" d="M137.7,24.4v9.9"/>
			<path fill="none" stroke="#3371B7" d="M126.8,23.5h10.8v2.7h-10.8C126.8,26.2,126.8,23.5,126.8,23.5z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="#3371B7" d="M170.8,23.1L170.8,23.1c-0.3,0-0.5,0-0.8,0c-2.1,0-4,1-5.3,2.5l-0.1,0l-0.1-0.1l-1-1.2l-0.3,3.4l3.4,0.3
				l-1.1-1.3l-0.1-0.1l0.1-0.1c1.1-1.5,3-2.3,5-2.1l0,0c3.2,0.3,5.5,3.1,5.2,6.3c-0.3,3-3.1,5.3-6.1,5.1c-3.1-0.2-5.4-2.9-5.3-6
				l-1.3-0.1c-0.2,3.8,2.6,7.1,6.3,7.4c3.9,0.3,7.3-2.6,7.6-6.5C177.2,26.8,174.4,23.5,170.8,23.1z"/>
			<path fill="#3371B7" d="M170.3,27.4c0-0.3-0.3-0.6-0.6-0.6s-0.6,0.3-0.6,0.6v3.2c0,0.2,0.1,0.3,0.2,0.4c0.1,0.1,0.3,0.2,0.4,0.2
				h2.4c0.4,0,0.6-0.3,0.6-0.6c0-0.4-0.3-0.6-0.6-0.6h-1.6h-0.2v-0.2L170.3,27.4L170.3,27.4z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#3371B7" d="M186.2,23.4h7.7c1.5,0,2.7,1.2,2.7,2.7v7.7c0,1.5-1.2,2.7-2.7,2.7h-7.7
				c-1.5,0-2.7-1.2-2.7-2.7v-7.7C183.4,24.6,184.7,23.4,186.2,23.4z"/>
			<ellipse fill="#3371B7" cx="186" cy="28.9" rx="0.7" ry="0.7"/>
			<ellipse fill="#3371B7" cx="194" cy="26.7" rx="0.7" ry="0.7"/>
			<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M186,33.3l0.4-0.3c0.4-0.3,1-0.3,1.5-0.1l1,0.4
				c0.5,0.2,1,0.2,1.5-0.1l0.8-0.5c0.4-0.3,1-0.3,1.5-0.1l1.8,0.8"/>
		</g>
	</g>
	<path fill="#3371B7" stroke="#3371B7" stroke-width="0.25" stroke-miterlimit="10" d="M156,24.3c-0.2-0.1-0.4-0.1-0.5,0
		c0,0-0.2,0.1-0.9,0.2c-0.7,0-2.4-0.1-3.8-0.6c-0.8-0.3-1.7-0.5-2.5-0.5c-0.2,0-0.4,0-0.5,0c-1.3,0-2.5,0.3-3.6,1
		c-0.2,0.1-0.2,0.2-0.2,0.4v11.6c0,0.3,0.1,0.5,0.3,0.5c0.6,0,0.5-0.4,0.5-0.6v-5.7c0.7-0.3,3.2-1.1,5.8-0.1
		c1.6,0.6,3.5,0.7,4.3,0.7c0.8,0,1.3-0.3,1.3-0.3c0.2-0.1,0.3-0.2,0.3-0.4v-5.7C156.2,24.6,156.1,24.4,156,24.3z M155.6,30.2
		c-0.1,0-0.7,0.1-1,0.1c-0.7,0-2.4-0.1-3.8-0.6c-2.5-1-5-0.5-6.2-0.1v-4.9c0.9-0.5,2.2-0.7,3.2-0.7c0.1,0,0.3,0,0.4,0
		c0.7,0,1.5,0.2,2.2,0.4c1.6,0.6,3.5,0.7,4.3,0.7c0.2,0,0.8,0,1-0.1V30.2z"/>
	<g>
		<path fill="none" stroke="#3371B7" d="M48.1,23.5h3.7c2.5,0,4.5,2,4.5,4.5c0,0.5-0.4,0.9-0.9,0.9H44.5c-0.5,0-0.9-0.4-0.9-0.9
			C43.6,25.5,45.6,23.5,48.1,23.5z"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M43.5,28.8c-0.2,0.1-0.5,1.2,0,1.5c1.4,1,8.5,0.8,11.3,0.6
			c0.8-0.1,1.6-0.4,1.7-1.2c0-0.3-0.1-0.6-0.6-0.9"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M43.5,30.6L43.3,31c-0.2,0.5,0.2,1,0.7,0.9c0.3-0.1,0.5,0,0.7,0.3
			l0.1,0.2c0.3,0.5,1,0.6,1.5,0.2l0,0c0.3-0.2,0.7-0.3,1-0.2l0.8,0.3c0.4,0.2,0.8,0.1,1.2,0l0.5-0.2c0.4-0.2,0.9-0.2,1.3,0l0.5,0.2
			c0.4,0.2,0.8,0.2,1.2,0l0.2-0.1c0.3-0.2,0.8-0.2,1.1,0.1l0.2,0.2c0.3,0.3,0.8,0.2,1-0.2l0.1-0.2c0.1-0.2,0-0.3,0.2-0.3
			c0.5,0,1.2-0.3,1.1-0.7l-0.4-1.1"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M43.5,32.2c-0.1,0.2-0.3,0.8,0,1.1c0.3,0.4,3,1.1,6.4,1.1
			c2.2,0,4.6-0.3,6-0.6c0.5-0.1,0.9-0.5,0.8-0.9c0-0.2-0.2-0.5-0.4-0.7"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M43.5,33.3c0,0.5,0.6,2.3,1.3,2.7c1.8,0.8,5.7,0.7,8.1,0.5
			c1.3-0.1,2.5-0.7,3.2-1.8c0.3-0.5,0.5-1,0.5-1.4"/>
		<ellipse fill="#3371B7" cx="51.6" cy="26.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#3371B7" cx="53" cy="25" rx="0.3" ry="0.4"/>
		<ellipse fill="#3371B7" cx="53" cy="27.2" rx="0.3" ry="0.4"/>
		<ellipse fill="#3371B7" cx="54.3" cy="26.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#3371B7" cx="50.9" cy="25" rx="0.3" ry="0.4"/>
	</g>
	<g>
		<path fill="none" stroke="#3371B7" d="M24.2,31v-7.6c0.1,0.1,0.8,0.9,2.8,3.1c2.5-1.6,5.6-0.7,6.9,0l2.4-3.1v7.1
			c0,1.2-0.1,2.5-0.9,3.4c-1,1.2-2.7,2.5-5.3,2.5c-2.9,0-4.5-1.5-5.3-2.9C24.2,32.9,24.2,32,24.2,31z"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M21.2,30l5.4,1.2"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M21.2,34.1l5.4-1.2"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M38.8,30l-5.4,1.2"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M38.8,34.1l-5.4-1.2"/>
		<path fill-rule="evenodd" clip-rule="evenodd" fill="#3371B7" d="M29.5,32.4L29,31.7c-0.2-0.3,0-0.6,0.3-0.6h1.4
			c0.3,0,0.5,0.4,0.3,0.6l-0.7,1l0,0c-0.7,1.2-2.6,1.1-3.1-0.3l-0.1-0.2c-0.1-0.2,0-0.4,0.2-0.5s0.4,0,0.5,0.2l0.1,0.2
			C28.3,32.8,29.1,32.9,29.5,32.4z"/>
		<path fill="none" stroke="#3371B7" stroke-linecap="round" d="M32.4,32.1l-0.1,0.2c-0.4,1-1.8,1.1-2.3,0.2"/>
		<ellipse fill="#3371B7" cx="27.6" cy="29.7" rx="0.7" ry="0.7"/>
		<ellipse fill="#3371B7" cx="32.4" cy="29.7" rx="0.7" ry="0.7"/>
	</g>
	<g>
		<path fill="#C0C0BF" stroke="#C0C0BF" stroke-width="0.1" stroke-miterlimit="10" d="M12.8,49.5c0.6,0,1.1-0.5,1.1-1.1
			c0-0.6-0.5-1.2-1.1-1.2c-0.6,0-1.2,0.5-1.2,1.2C11.6,49,12.2,49.5,12.8,49.5z M12.8,48c0.2,0,0.4,0.2,0.4,0.4
			c0,0.2-0.2,0.4-0.4,0.4c-0.2,0-0.4-0.2-0.4-0.4C12.4,48.1,12.6,48,12.8,48z"/>
		<path fill="#C0C0BF" stroke="#C0C0BF" stroke-width="0.1" stroke-miterlimit="10" d="M14.6,51.2c-0.1-0.1-0.2-0.2-0.3-0.2H5.7
			c-0.1,0-0.2,0.1-0.3,0.2c-0.1,0.1-0.1,0.2,0,0.4c0.7,2,2.5,3.3,4.6,3.3s3.9-1.3,4.6-3.3C14.7,51.5,14.7,51.3,14.6,51.2z M10,54.1
			c-1.6,0-3-0.9-3.7-2.2h7.3C13,53.2,11.6,54.1,10,54.1z"/>
		<path fill="#C0C0BF" stroke="#C0C0BF" stroke-width="0.1" stroke-miterlimit="10" d="M7.2,49.5c0.6,0,1.2-0.5,1.2-1.1
			c0-0.6-0.5-1.2-1.2-1.2c-0.6,0-1.1,0.5-1.1,1.2C6.1,49,6.6,49.5,7.2,49.5z M7.2,48c0.2,0,0.4,0.2,0.4,0.4c0,0.2-0.2,0.4-0.4,0.4
			c-0.2,0-0.4-0.2-0.4-0.4C6.8,48.1,7,48,7.2,48z"/>
		<path fill="#C0C0BF" stroke="#C0C0BF" stroke-width="0.1" stroke-miterlimit="10" d="M10,43c-3.8,0-7,3.1-7,7c0,3.8,3.1,7,7,7
			s7-3.1,7-7C17,46.2,13.8,43,10,43z M10,56.2c-3.4,0-6.2-2.8-6.2-6.2c0-3.4,2.8-6.2,6.2-6.2s6.2,2.8,6.2,6.2
			C16.2,53.4,13.4,56.2,10,56.2z"/>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#C0C0BF" d="M64.1,53.4l2.3,0c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.3
				c-0.2,0-0.4-0.2-0.4-0.4v-2.1C63.7,53.6,63.8,53.4,64.1,53.4z"/>
			<path fill="none" stroke="#C0C0BF" d="M73.5,53.5h2.4c0.2,0,0.4,0.2,0.4,0.4v2c0,0.2-0.2,0.4-0.4,0.4h-2.4
				c-0.2,0-0.4-0.2-0.4-0.4l0-2C73.1,53.6,73.3,53.5,73.5,53.5z"/>
			<path fill="none" stroke="#C0C0BF" d="M63.7,48.4h12.6v5H63.7V48.4z"/>
			<path fill="none" stroke="#C0C0BF" d="M65.5,43.6h8.9c1,0,1.9,0.8,1.9,1.9v3.1H63.7v-3.1C63.7,44.5,64.5,43.6,65.5,43.6z"/>
			<ellipse fill="#C0C0BF" cx="66.2" cy="50.9" rx="0.9" ry="1"/>
			<ellipse fill="#C0C0BF" cx="73.8" cy="50.9" rx="0.9" ry="1"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#C0C0BF" d="M96.4,50c0,3.6-2.9,6.5-6.4,6.5s-6.4-2.9-6.4-6.5s2.9-6.5,6.4-6.5S96.4,46.4,96.4,50z"/>
			<path fill="none" stroke="#C0C0BF" d="M96.3,48.6c0,0,0,0.1,0,0.1c-0.9,0.1-2.9,0.1-4.6-1.2c-1.1-0.8-2-1.7-2.6-2.5
				c-0.3-0.4-0.6-0.8-0.7-1c-0.1-0.1-0.1-0.2-0.1-0.2c0.5-0.1,1.2-0.2,2-0.2c1.2,0,2.5,0.3,3.5,1.1c1,0.8,1.7,1.8,2.1,2.8
				C96.1,47.9,96.2,48.3,96.3,48.6z"/>
			<path fill="none" stroke="#C0C0BF" d="M84,52.2c0,0,0-0.1,0-0.1c0.9-0.2,2.9-0.4,4.7,0.6c1.1,0.7,1.9,1.5,2.4,2.3
				c0.4,0.5,0.6,1,0.7,1.3c-0.4,0.1-1,0.2-1.7,0.3c-1,0-2.1-0.1-3.2-0.8s-1.9-1.6-2.4-2.5C84.2,52.8,84.1,52.5,84,52.2z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M116.3,46.8l-1.4,2l-0.8-0.8l-0.6-0.7l0,0.9l-0.1,8.2h-6.8
				l-0.1-8.2l0-0.9l-0.6,0.7l-0.8,0.8l-1.4-2l2.6-2.9c0.1-0.1,0.2-0.1,0.3-0.1h1.3l0.4,0.7c0.7,1.3,2.6,1.3,3.3-0.1l0.3-0.6h1.2
				c0.1,0,0.2,0,0.3,0.1l0.3-0.3l-0.3,0.3L116.3,46.8z"/>
			<path fill="none" stroke="#C0C0BF" d="M110.1,47.7h2v0.9c0,0.4-0.4,0.7-1,0.7c-0.6,0-1-0.3-1-0.7L110.1,47.7L110.1,47.7z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#C0C0BF" d="M126.8,54.3c0,1.2-1,2.2-2.2,2.2s-2.2-1-2.2-2.2s1-2.2,2.2-2.2S126.8,53.1,126.8,54.3z"/>
			<path fill="none" stroke="#C0C0BF" d="M137.6,54.3c0,1.2-1,2.2-2.2,2.2c-1.2,0-2.2-1-2.2-2.2s1-2.2,2.2-2.2
				C136.6,52.1,137.6,53.1,137.6,54.3z"/>
			<path fill="none" stroke="#C0C0BF" d="M126.8,44.4v9.9"/>
			<path fill="none" stroke="#C0C0BF" d="M137.7,44.4v9.9"/>
			<path fill="none" stroke="#C0C0BF" d="M126.8,43.5h10.8v2.7h-10.8C126.8,46.2,126.8,43.5,126.8,43.5z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="#C0C0BF" d="M170.8,43.1L170.8,43.1c-0.3,0-0.5,0-0.8,0c-2.1,0-4,1-5.3,2.5l-0.1,0l-0.1-0.1l-1-1.2l-0.3,3.4l3.4,0.3
				l-1.1-1.3l-0.1-0.1l0.1-0.1c1.1-1.5,3-2.3,5-2.1l0,0c3.2,0.3,5.5,3.1,5.2,6.3c-0.3,3-3.1,5.3-6.1,5.1c-3.1-0.2-5.4-2.9-5.3-6
				l-1.3-0.1c-0.2,3.8,2.6,7.1,6.3,7.4c3.9,0.3,7.3-2.6,7.6-6.5C177.2,46.8,174.4,43.5,170.8,43.1z"/>
			<path fill="#C0C0BF" d="M170.3,47.4c0-0.3-0.3-0.6-0.6-0.6s-0.6,0.3-0.6,0.6v3.2c0,0.2,0.1,0.3,0.2,0.4c0.1,0.1,0.3,0.2,0.4,0.2
				h2.4c0.4,0,0.6-0.3,0.6-0.6c0-0.3-0.3-0.6-0.6-0.6h-1.6h-0.2v-0.2L170.3,47.4L170.3,47.4z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#C0C0BF" d="M186.2,43.4h7.7c1.5,0,2.7,1.2,2.7,2.7v7.7c0,1.5-1.2,2.7-2.7,2.7h-7.7
				c-1.5,0-2.7-1.2-2.7-2.7v-7.7C183.4,44.7,184.7,43.4,186.2,43.4z"/>
			<ellipse fill="#C0C0BF" cx="186" cy="48.9" rx="0.7" ry="0.7"/>
			<ellipse fill="#C0C0BF" cx="194" cy="46.7" rx="0.7" ry="0.7"/>
			<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M186,53.3l0.4-0.3c0.4-0.3,1-0.3,1.5-0.1l1,0.4
				c0.5,0.2,1,0.2,1.5-0.1l0.8-0.5c0.4-0.3,1-0.3,1.5-0.1l1.8,0.8"/>
		</g>
	</g>
	<path fill="#C0C0BF" stroke="#C0C0BF" stroke-width="0.25" stroke-miterlimit="10" d="M156,44.3c-0.2-0.1-0.4-0.1-0.5,0
		c0,0-0.2,0.1-0.9,0.2c-0.7,0-2.4-0.1-3.8-0.6c-0.8-0.3-1.7-0.5-2.5-0.5c-0.2,0-0.4,0-0.5,0c-1.3,0-2.5,0.3-3.6,1
		c-0.2,0.1-0.2,0.2-0.2,0.4v11.6c0,0.3,0.1,0.5,0.3,0.5c0.6,0,0.5-0.4,0.5-0.6v-5.7c0.7-0.3,3.2-1.1,5.8-0.1
		c1.6,0.6,3.5,0.7,4.3,0.7c0.8,0,1.3-0.3,1.3-0.3c0.2-0.1,0.3-0.2,0.3-0.4v-5.7C156.2,44.6,156.1,44.4,156,44.3z M155.6,50.2
		c-0.1,0-0.7,0.1-1,0.1c-0.7,0-2.4-0.1-3.8-0.6c-2.5-1-5-0.5-6.2-0.1v-4.9c0.9-0.5,2.2-0.7,3.2-0.7c0.1,0,0.3,0,0.4,0
		c0.7,0,1.5,0.2,2.2,0.4c1.6,0.6,3.5,0.7,4.3,0.7c0.2,0,0.8,0,1-0.1V50.2z"/>
	<g>
		<path fill="none" stroke="#C0C0BF" d="M48.1,43.5h3.7c2.5,0,4.5,2,4.5,4.5c0,0.5-0.4,0.9-0.9,0.9H44.5c-0.5,0-0.9-0.4-0.9-0.9
			C43.6,45.5,45.6,43.5,48.1,43.5z"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M43.5,48.8c-0.2,0.1-0.5,1.2,0,1.5c1.4,1,8.5,0.8,11.3,0.6
			c0.8-0.1,1.6-0.4,1.7-1.2c0-0.3-0.1-0.6-0.6-0.9"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M43.5,50.6L43.3,51c-0.2,0.5,0.2,1,0.7,0.9c0.3-0.1,0.5,0,0.7,0.3
			l0.1,0.2c0.3,0.5,1,0.6,1.5,0.2l0,0c0.3-0.2,0.7-0.3,1-0.2l0.8,0.3c0.4,0.2,0.8,0.1,1.2,0l0.5-0.2c0.4-0.2,0.9-0.2,1.3,0l0.5,0.2
			c0.4,0.2,0.8,0.2,1.2,0l0.2-0.1c0.3-0.2,0.8-0.2,1.1,0.1l0.2,0.2c0.3,0.3,0.8,0.2,1-0.2l0.1-0.2c0.1-0.2,0-0.3,0.2-0.3
			c0.5,0,1.2-0.3,1.1-0.7l-0.4-1.1"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M43.5,52.2c-0.1,0.2-0.3,0.8,0,1.1c0.3,0.4,3,1.1,6.4,1.1
			c2.2,0,4.6-0.3,6-0.6c0.5-0.1,0.9-0.5,0.8-0.9c0-0.2-0.2-0.5-0.4-0.7"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M43.5,53.3c0,0.5,0.6,2.3,1.3,2.7c1.8,0.8,5.7,0.7,8.1,0.5
			c1.3-0.1,2.5-0.7,3.2-1.8c0.3-0.5,0.5-1,0.5-1.4"/>
		<ellipse fill="#C0C0BF" cx="51.6" cy="46.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#C0C0BF" cx="53" cy="45" rx="0.3" ry="0.4"/>
		<ellipse fill="#C0C0BF" cx="53" cy="47.2" rx="0.3" ry="0.4"/>
		<ellipse fill="#C0C0BF" cx="54.3" cy="46.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#C0C0BF" cx="50.9" cy="45" rx="0.3" ry="0.4"/>
	</g>
	<g>
		<path fill="none" stroke="#C0C0BF" d="M24.2,51v-7.6c0.1,0.1,0.8,0.9,2.8,3.1c2.5-1.7,5.6-0.7,6.9,0l2.4-3.1v7.1
			c0,1.2-0.1,2.5-0.9,3.4c-1,1.2-2.7,2.5-5.3,2.5c-2.9,0-4.5-1.5-5.3-2.9C24.2,52.9,24.2,52,24.2,51z"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M21.2,50l5.4,1.2"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M21.2,54.1l5.4-1.2"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M38.8,50l-5.4,1.2"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M38.8,54.1l-5.4-1.2"/>
		<path fill-rule="evenodd" clip-rule="evenodd" fill="#C0C0BF" d="M29.5,52.4L29,51.7c-0.2-0.3,0-0.6,0.3-0.6h1.4
			c0.3,0,0.5,0.4,0.3,0.6l-0.7,1l0,0c-0.7,1.2-2.6,1.1-3.1-0.3l-0.1-0.2c-0.1-0.2,0-0.4,0.2-0.5s0.4,0,0.5,0.2l0.1,0.2
			C28.3,52.8,29.1,52.9,29.5,52.4z"/>
		<path fill="none" stroke="#C0C0BF" stroke-linecap="round" d="M32.4,52.1l-0.1,0.2c-0.4,1-1.8,1.1-2.3,0.2"/>
		<ellipse fill="#C0C0BF" cx="27.6" cy="49.7" rx="0.7" ry="0.7"/>
		<ellipse fill="#C0C0BF" cx="32.4" cy="49.7" rx="0.7" ry="0.7"/>
	</g>
	<g>
		<path fill="#6AA9DD" stroke="#6AA9DD" stroke-width="0.1" stroke-miterlimit="10" d="M14.6,71.2c-0.1-0.1-0.2-0.2-0.3-0.2H5.7
			c-0.1,0-0.2,0.1-0.3,0.2c-0.1,0.1-0.1,0.2,0,0.4c0.7,2,2.5,3.3,4.6,3.3s3.9-1.3,4.6-3.3C14.7,71.5,14.7,71.3,14.6,71.2z M10,74.1
			c-1.6,0-3-0.9-3.7-2.2h7.3C13,73.2,11.6,74.1,10,74.1z"/>
		<path fill="#6AA9DD" stroke="#6AA9DD" stroke-width="0.1" stroke-miterlimit="10" d="M12.8,69.5c0.6,0,1.1-0.5,1.1-1.1
			c0-0.6-0.5-1.2-1.1-1.2c-0.6,0-1.2,0.5-1.2,1.2C11.6,69,12.2,69.5,12.8,69.5z M12.8,68c0.2,0,0.4,0.2,0.4,0.4
			c0,0.2-0.2,0.4-0.4,0.4c-0.2,0-0.4-0.2-0.4-0.4C12.4,68.1,12.6,68,12.8,68z"/>
		<path fill="#6AA9DD" stroke="#6AA9DD" stroke-width="0.1" stroke-miterlimit="10" d="M7.2,69.5c0.6,0,1.2-0.5,1.2-1.1
			c0-0.6-0.5-1.2-1.2-1.2c-0.6,0-1.1,0.5-1.1,1.2C6.1,69,6.6,69.5,7.2,69.5z M7.2,68c0.2,0,0.4,0.2,0.4,0.4c0,0.2-0.2,0.4-0.4,0.4
			c-0.2,0-0.4-0.2-0.4-0.4C6.8,68.1,7,68,7.2,68z"/>
		<path fill="#6AA9DD" stroke="#6AA9DD" stroke-width="0.1" stroke-miterlimit="10" d="M10,63c-3.8,0-7,3.1-7,7c0,3.8,3.1,7,7,7
			s7-3.1,7-7C17,66.2,13.8,63,10,63z M10,76.2c-3.4,0-6.2-2.8-6.2-6.2c0-3.4,2.8-6.2,6.2-6.2s6.2,2.8,6.2,6.2
			C16.2,73.4,13.4,76.2,10,76.2z"/>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#6AA9DD" d="M64.1,73.4l2.3,0c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.3
				c-0.2,0-0.4-0.2-0.4-0.4v-2.1C63.7,73.6,63.8,73.4,64.1,73.4z"/>
			<path fill="none" stroke="#6AA9DD" d="M73.5,73.5h2.4c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.4
				c-0.2,0-0.4-0.2-0.4-0.4l0-2.1C73.1,73.6,73.3,73.5,73.5,73.5z"/>
			<path fill="none" stroke="#6AA9DD" d="M63.7,68.4h12.6v5H63.7V68.4z"/>
			<path fill="none" stroke="#6AA9DD" d="M65.5,63.6h8.9c1,0,1.9,0.8,1.9,1.9v3.1H63.7v-3.1C63.7,64.5,64.5,63.6,65.5,63.6z"/>
			<ellipse fill="#6AA9DD" cx="66.2" cy="70.9" rx="0.9" ry="0.9"/>
			<ellipse fill="#6AA9DD" cx="73.8" cy="70.9" rx="0.9" ry="0.9"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#6AA9DD" d="M96.4,70c0,3.6-2.9,6.5-6.4,6.5s-6.4-2.9-6.4-6.5s2.9-6.5,6.4-6.5S96.4,66.4,96.4,70z"/>
			<path fill="none" stroke="#6AA9DD" d="M96.3,68.6c0,0,0,0.1,0,0.1c-0.9,0.1-2.9,0.1-4.6-1.2c-1.1-0.8-2-1.7-2.6-2.5
				c-0.3-0.4-0.6-0.8-0.7-1.1c-0.1-0.1-0.1-0.2-0.1-0.2c0.5-0.1,1.2-0.2,2-0.2c1.2,0,2.5,0.3,3.5,1.1c1,0.8,1.7,1.8,2.1,2.8
				C96.1,67.9,96.2,68.3,96.3,68.6z"/>
			<path fill="none" stroke="#6AA9DD" d="M84,72.2c0,0,0-0.1,0-0.1c0.9-0.2,2.9-0.4,4.7,0.6c1.1,0.7,1.9,1.5,2.4,2.3
				c0.4,0.5,0.6,1,0.7,1.3c-0.4,0.1-1,0.2-1.7,0.3c-1,0-2.1-0.1-3.2-0.8s-1.9-1.6-2.4-2.5C84.2,72.8,84.1,72.4,84,72.2z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M116.3,66.8l-1.4,2l-0.8-0.8l-0.6-0.7l0,0.9l-0.1,8.2h-6.8
				l-0.1-8.2l0-0.9l-0.6,0.7l-0.8,0.8l-1.4-2l2.6-2.9c0.1-0.1,0.2-0.1,0.3-0.1h1.3l0.4,0.7c0.7,1.3,2.6,1.3,3.3-0.1l0.3-0.6h1.2
				c0.1,0,0.2,0,0.3,0.1l0.3-0.3l-0.3,0.3L116.3,66.8z"/>
			<path fill="none" stroke="#6AA9DD" d="M110.1,67.7h2v0.9c0,0.4-0.4,0.7-1,0.7c-0.6,0-1-0.3-1-0.7L110.1,67.7L110.1,67.7z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#6AA9DD" d="M126.8,74.3c0,1.2-1,2.2-2.2,2.2s-2.2-1-2.2-2.2s1-2.2,2.2-2.2S126.8,73.1,126.8,74.3z"/>
			<path fill="none" stroke="#6AA9DD" d="M137.6,74.3c0,1.2-1,2.2-2.2,2.2c-1.2,0-2.2-1-2.2-2.2s1-2.2,2.2-2.2
				C136.6,72.1,137.6,73.1,137.6,74.3z"/>
			<path fill="none" stroke="#6AA9DD" d="M126.8,64.4v9.9"/>
			<path fill="none" stroke="#6AA9DD" d="M137.7,64.4v9.9"/>
			<path fill="none" stroke="#6AA9DD" d="M126.8,63.5h10.8v2.7h-10.8C126.8,66.2,126.8,63.5,126.8,63.5z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="#6AA9DD" d="M170.8,63.1L170.8,63.1c-0.3,0-0.5,0-0.8,0c-2.1,0-4,1-5.3,2.5l-0.1,0l-0.1-0.1l-1-1.2l-0.3,3.4l3.4,0.3
				l-1.1-1.3l-0.1-0.1l0.1-0.1c1.1-1.4,3-2.3,5-2.1l0,0c3.2,0.3,5.5,3.1,5.2,6.3c-0.3,3-3.1,5.3-6.1,5.1c-3.1-0.2-5.4-2.9-5.3-6
				l-1.3-0.1c-0.2,3.8,2.6,7.1,6.3,7.4c3.9,0.3,7.3-2.6,7.6-6.5C177.2,66.8,174.4,63.5,170.8,63.1z"/>
			<path fill="#6AA9DD" d="M170.3,67.4c0-0.3-0.3-0.6-0.6-0.6s-0.6,0.3-0.6,0.6v3.2c0,0.2,0.1,0.3,0.2,0.4c0.1,0.1,0.3,0.2,0.4,0.2
				h2.4c0.4,0,0.6-0.3,0.6-0.6S172.4,70,172,70h-1.6h-0.2v-0.2L170.3,67.4L170.3,67.4z"/>
		</g>
	</g>
	<g>
		<g>
			<path fill="none" stroke="#6AA9DD" d="M186.2,63.4h7.7c1.5,0,2.7,1.2,2.7,2.7v7.7c0,1.5-1.2,2.7-2.7,2.7h-7.7
				c-1.5,0-2.7-1.2-2.7-2.7v-7.7C183.4,64.7,184.7,63.4,186.2,63.4z"/>
			<ellipse fill="#6AA9DD" cx="186" cy="68.9" rx="0.7" ry="0.7"/>
			<ellipse fill="#6AA9DD" cx="194" cy="66.7" rx="0.7" ry="0.7"/>
			<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M186,73.3l0.4-0.3c0.4-0.3,1-0.3,1.5-0.1l1,0.4
				c0.5,0.2,1,0.2,1.5-0.1l0.8-0.5c0.4-0.3,1-0.3,1.5-0.1l1.8,0.8"/>
		</g>
	</g>
	<path fill="#6AA9DD" stroke="#6AA9DD" stroke-width="0.25" stroke-miterlimit="10" d="M156,64.3c-0.2-0.1-0.4-0.1-0.5,0
		c0,0-0.2,0.1-0.9,0.2c-0.7,0-2.4-0.1-3.8-0.6c-0.8-0.3-1.7-0.5-2.5-0.5c-0.2,0-0.4,0-0.5,0c-1.3,0-2.5,0.3-3.6,1
		c-0.2,0.1-0.2,0.2-0.2,0.4v11.6c0,0.3,0.1,0.5,0.3,0.5c0.6,0,0.5-0.4,0.5-0.6v-5.7c0.7-0.3,3.2-1.1,5.8-0.1
		c1.6,0.6,3.5,0.7,4.3,0.7c0.8,0,1.3-0.3,1.3-0.3c0.2-0.1,0.3-0.2,0.3-0.4v-5.7C156.2,64.6,156.1,64.4,156,64.3z M155.6,70.2
		c-0.1,0-0.7,0.1-1,0.1c-0.7,0-2.4-0.1-3.8-0.6c-2.5-1-5-0.5-6.2-0.1v-4.9c0.9-0.5,2.2-0.7,3.2-0.7c0.1,0,0.3,0,0.4,0
		c0.7,0,1.5,0.2,2.2,0.4c1.6,0.6,3.5,0.7,4.3,0.7c0.2,0,0.8,0,1-0.1V70.2z"/>
	<g>
		<path fill="none" stroke="#6AA9DD" d="M48.1,63.5h3.7c2.5,0,4.5,2,4.5,4.5c0,0.5-0.4,0.9-0.9,0.9H44.5c-0.5,0-0.9-0.4-0.9-0.9
			C43.6,65.5,45.6,63.5,48.1,63.5z"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M43.5,68.8c-0.2,0.1-0.5,1.2,0,1.5c1.4,0.9,8.5,0.8,11.3,0.6
			c0.8-0.1,1.6-0.4,1.7-1.2c0-0.3-0.1-0.6-0.6-0.9"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M43.5,70.6L43.3,71c-0.2,0.5,0.2,1,0.7,0.9c0.3-0.1,0.5,0.1,0.7,0.3
			l0.1,0.2c0.3,0.5,1,0.6,1.5,0.2l0,0c0.3-0.2,0.7-0.3,1-0.2l0.8,0.3c0.4,0.2,0.8,0.1,1.2,0l0.5-0.2c0.4-0.2,0.9-0.2,1.3,0l0.5,0.2
			c0.4,0.2,0.8,0.2,1.2-0.1l0.2-0.1c0.3-0.2,0.8-0.2,1.1,0.1l0.2,0.2c0.3,0.3,0.8,0.2,1-0.2l0.1-0.2c0.1-0.2,0-0.3,0.2-0.3
			c0.5,0,1.2-0.3,1.1-0.7l-0.4-1.1"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M43.5,72.2c-0.1,0.2-0.3,0.8,0,1.1c0.3,0.4,3,1.1,6.4,1.1
			c2.2,0,4.6-0.3,6-0.6c0.5-0.1,0.9-0.4,0.8-0.9c0-0.2-0.2-0.5-0.4-0.7"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M43.5,73.3c0,0.5,0.6,2.3,1.3,2.7c1.8,0.8,5.7,0.7,8.1,0.5
			c1.3-0.1,2.5-0.7,3.2-1.8c0.3-0.5,0.5-1,0.5-1.4"/>
		<ellipse fill="#6AA9DD" cx="51.6" cy="66.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#6AA9DD" cx="53" cy="65" rx="0.3" ry="0.4"/>
		<ellipse fill="#6AA9DD" cx="53" cy="67.2" rx="0.3" ry="0.4"/>
		<ellipse fill="#6AA9DD" cx="54.3" cy="66.5" rx="0.3" ry="0.4"/>
		<ellipse fill="#6AA9DD" cx="50.9" cy="65" rx="0.3" ry="0.4"/>
	</g>
	<g>
		<path fill="none" stroke="#6AA9DD" d="M24.2,71v-7.6c0.1,0.1,0.8,0.9,2.8,3.1c2.5-1.7,5.6-0.7,6.9,0l2.4-3.1v7.1
			c0,1.2-0.1,2.5-0.9,3.4c-1,1.2-2.7,2.5-5.3,2.5c-2.9,0-4.5-1.5-5.3-2.9C24.2,72.9,24.2,72,24.2,71z"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M21.2,70.1l5.4,1.2"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M21.2,74.1l5.4-1.2"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M38.8,70.1l-5.4,1.2"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M38.8,74.1l-5.4-1.2"/>
		<path fill-rule="evenodd" clip-rule="evenodd" fill="#6AA9DD" d="M29.5,72.4L29,71.7c-0.2-0.3,0-0.6,0.3-0.6h1.4
			c0.3,0,0.5,0.4,0.3,0.6l-0.7,1l0,0c-0.7,1.2-2.6,1.1-3.1-0.3l-0.1-0.2c-0.1-0.2,0-0.4,0.2-0.5c0.2-0.1,0.4,0,0.5,0.2l0.1,0.2
			C28.3,72.8,29.1,72.9,29.5,72.4z"/>
		<path fill="none" stroke="#6AA9DD" stroke-linecap="round" d="M32.4,72.1l-0.1,0.2c-0.4,1-1.8,1.1-2.3,0.2"/>
		<ellipse fill="#6AA9DD" cx="27.6" cy="69.7" rx="0.7" ry="0.7"/>
		<ellipse fill="#6AA9DD" cx="32.4" cy="69.7" rx="0.7" ry="0.7"/>
	</g>
</g>
<g>
	<path fill="#868686" stroke="#868686" stroke-width="0.1" stroke-miterlimit="10" d="M12.8,9.5c0.6,0,1.1-0.5,1.1-1.2
		c0-0.6-0.5-1.1-1.1-1.1c-0.6,0-1.2,0.5-1.2,1.1S12.2,9.5,12.8,9.5z M12.8,7.9c0.2,0,0.4,0.2,0.4,0.4c0,0.2-0.2,0.4-0.4,0.4
		c-0.2,0-0.4-0.2-0.4-0.4C12.4,8.1,12.6,7.9,12.8,7.9z"/>
	<path fill="#868686" stroke="#868686" stroke-width="0.1" stroke-miterlimit="10" d="M7.2,9.5c0.6,0,1.2-0.5,1.2-1.2
		c0-0.6-0.5-1.1-1.2-1.1c-0.6,0-1.1,0.5-1.1,1.1S6.6,9.5,7.2,9.5z M7.2,7.9c0.2,0,0.4,0.2,0.4,0.4c0,0.2-0.2,0.4-0.4,0.4
		C7,8.7,6.8,8.5,6.8,8.3C6.8,8.1,7,7.9,7.2,7.9z"/>
	<path fill="#868686" stroke="#868686" stroke-width="0.1" stroke-miterlimit="10" d="M14.6,11.2c-0.1-0.1-0.2-0.2-0.3-0.2H5.7
		c-0.1,0-0.2,0.1-0.3,0.2c-0.1,0.1-0.1,0.2,0,0.4c0.7,2,2.5,3.3,4.6,3.3s3.9-1.3,4.6-3.3C14.7,11.4,14.7,11.3,14.6,11.2z M10,14.1
		c-1.6,0-3-0.9-3.7-2.2h7.3C13,13.2,11.6,14.1,10,14.1z"/>
	<path fill="#868686" stroke="#868686" stroke-width="0.1" stroke-miterlimit="10" d="M10,3c-3.8,0-7,3.1-7,7s3.1,7,7,7s7-3.1,7-7
		S13.8,3,10,3z M10,16.2c-3.4,0-6.2-2.8-6.2-6.2S6.6,3.8,10,3.8s6.2,2.8,6.2,6.2S13.4,16.2,10,16.2z"/>
</g>
<g id="Car_00000018935395085413413750000000825273877842775715_">
	<g>
		<g>
			<path fill="none" stroke="#868686" d="M64.1,13.4l2.3,0c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.3
				c-0.2,0-0.4-0.2-0.4-0.4v-2.1C63.7,13.6,63.8,13.4,64.1,13.4z"/>
			<path fill="none" stroke="#868686" d="M73.5,13.4h2.4c0.2,0,0.4,0.2,0.4,0.4v2.1c0,0.2-0.2,0.4-0.4,0.4h-2.4
				c-0.2,0-0.4-0.2-0.4-0.4l0-2.1C73.1,13.6,73.3,13.4,73.5,13.4z"/>
			<path fill="none" stroke="#868686" d="M63.7,8.4h12.6v5H63.7V8.4z"/>
			<path fill="none" stroke="#868686" d="M65.5,3.6h8.9c1,0,1.9,0.8,1.9,1.9v3.1H63.7V5.5C63.7,4.4,64.5,3.6,65.5,3.6z"/>
			<ellipse fill="#868686" cx="66.2" cy="10.9" rx="0.9" ry="0.9"/>
			<ellipse fill="#868686" cx="73.8" cy="10.9" rx="0.9" ry="0.9"/>
		</g>
	</g>
</g>
<g id="Activities">
	<g>
		<g>
			<path fill="none" stroke="#868686" d="M96.4,10c0,3.6-2.9,6.5-6.4,6.5s-6.4-2.9-6.4-6.5s2.9-6.5,6.4-6.5S96.4,6.4,96.4,10z"/>
			<path fill="none" stroke="#868686" d="M96.3,8.6c0,0,0,0.1,0,0.1c-0.9,0.1-2.9,0.1-4.6-1.2c-1.1-0.8-2-1.7-2.6-2.5
				c-0.3-0.4-0.6-0.8-0.7-1.1c-0.1-0.1-0.1-0.2-0.1-0.2c0.5-0.1,1.2-0.2,2-0.2c1.2,0,2.5,0.3,3.5,1.1c1,0.8,1.7,1.8,2.1,2.8
				C96.1,7.9,96.2,8.3,96.3,8.6z"/>
			<path fill="none" stroke="#868686" d="M84,12.1c0,0,0-0.1,0-0.1c0.9-0.2,2.9-0.4,4.7,0.6c1.1,0.6,1.9,1.5,2.4,2.3
				c0.4,0.5,0.6,1,0.7,1.3c-0.4,0.1-1,0.2-1.7,0.3c-1,0-2.1-0.1-3.2-0.8c-1.1-0.6-1.9-1.6-2.4-2.5C84.2,12.8,84.1,12.4,84,12.1z"/>
		</g>
	</g>
</g>
<g id="Objects_00000064312371739130315250000010225889032228632975_">
	<g>
		<g>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M116.3,6.8l-1.4,2L114.1,8l-0.6-0.7l0,0.9l-0.1,8.2h-6.8l-0.1-8.2
				l0-0.9L105.9,8l-0.8,0.8l-1.4-2l2.6-2.9c0.1-0.1,0.2-0.1,0.3-0.1h1.3l0.4,0.7c0.7,1.3,2.6,1.3,3.3-0.1l0.3-0.6h1.2
				c0.1,0,0.2,0,0.3,0.1l0.3-0.3l-0.3,0.3L116.3,6.8z"/>
			<path fill="none" stroke="#868686" d="M110.1,7.7h2v0.9c0,0.4-0.4,0.7-1,0.7c-0.6,0-1-0.3-1-0.7L110.1,7.7L110.1,7.7z"/>
		</g>
	</g>
</g>
<g id="Symbols_00000096746907869929921180000006484812830282582416_">
	<g>
		<g>
			<path fill="none" stroke="#868686" d="M126.8,14.3c0,1.2-1,2.2-2.2,2.2s-2.2-1-2.2-2.2s1-2.2,2.2-2.2S126.8,13.1,126.8,14.3z"/>
			<path fill="none" stroke="#868686" d="M137.6,14.3c0,1.2-1,2.2-2.2,2.2c-1.2,0-2.2-1-2.2-2.2s1-2.2,2.2-2.2
				C136.6,12.1,137.6,13.1,137.6,14.3z"/>
			<path fill="none" stroke="#868686" d="M126.8,4.4v9.9"/>
			<path fill="none" stroke="#868686" d="M137.7,4.4v9.9"/>
			<path fill="none" stroke="#868686" d="M126.8,3.5h10.8v2.7h-10.8C126.8,6.2,126.8,3.5,126.8,3.5z"/>
		</g>
	</g>
</g>
<g id="Recents">
	<g>
		<g>
			<path fill="#868686" d="M170.8,3.1L170.8,3.1c-0.3,0-0.5,0-0.8,0c-2.1,0-4,1-5.3,2.5l-0.1,0l-0.1-0.1l-1-1.2l-0.3,3.4l3.4,0.3
				l-1.1-1.3l-0.1-0.1l0.1-0.1c1.1-1.4,3-2.3,5-2.1l0,0c3.2,0.3,5.5,3.1,5.2,6.3c-0.3,3-3.1,5.3-6.1,5.1c-3.1-0.2-5.4-2.9-5.3-6
				L163,9.5c-0.2,3.8,2.6,7.1,6.3,7.4c3.9,0.4,7.3-2.6,7.6-6.5C177.2,6.8,174.4,3.5,170.8,3.1z"/>
			<path fill="#868686" d="M170.3,7.4c0-0.3-0.3-0.6-0.6-0.6S169,7.1,169,7.4v3.2c0,0.2,0.1,0.3,0.2,0.4c0.1,0.1,0.3,0.2,0.4,0.2
				h2.4c0.4,0,0.6-0.3,0.6-0.6s-0.3-0.6-0.6-0.6h-1.6h-0.2V9.8L170.3,7.4L170.3,7.4z"/>
		</g>
	</g>
</g>
<g id="Custom_00000181087229434302336780000005152277945940674488_">
	<g>
		<g>
			<path fill="none" stroke="#868686" d="M186.2,3.4h7.7c1.5,0,2.7,1.2,2.7,2.7v7.7c0,1.5-1.2,2.7-2.7,2.7h-7.7
				c-1.5,0-2.7-1.2-2.7-2.7V6.1C183.4,4.6,184.7,3.4,186.2,3.4z"/>
			<ellipse fill="#868686" cx="186" cy="8.9" rx="0.7" ry="0.7"/>
			<ellipse fill="#868686" cx="194" cy="6.7" rx="0.7" ry="0.7"/>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M186,13.3l0.4-0.3c0.4-0.3,1-0.3,1.5-0.1l1,0.4
				c0.5,0.2,1,0.2,1.5-0.1l0.8-0.5c0.4-0.3,1-0.3,1.5-0.1l1.8,0.8"/>
		</g>
	</g>
</g>
<path fill="#868686" stroke="#868686" stroke-width="0.25" stroke-miterlimit="10" d="M156,4.3c-0.2-0.1-0.4-0.1-0.5,0
	c0,0-0.2,0.1-0.9,0.1c-0.7,0-2.4-0.1-3.8-0.6c-0.8-0.3-1.7-0.5-2.5-0.5c-0.2,0-0.4,0-0.5,0c-1.3,0-2.5,0.3-3.6,1
	c-0.2,0.1-0.2,0.2-0.2,0.4v11.6c0,0.3,0.1,0.5,0.3,0.5c0.6,0,0.5-0.4,0.5-0.6v-5.7c0.7-0.3,3.2-1.1,5.8-0.1c1.6,0.6,3.5,0.7,4.3,0.7
	c0.8,0,1.3-0.3,1.3-0.3c0.2-0.1,0.3-0.2,0.3-0.4V4.7C156.2,4.5,156.1,4.4,156,4.3z M155.6,10.2c-0.1,0-0.7,0.1-1,0.1
	c-0.7,0-2.4-0.1-3.8-0.6c-2.5-1-5-0.5-6.2-0.1V4.7c0.9-0.5,2.2-0.7,3.2-0.7c0.1,0,0.3,0,0.4,0c0.7,0,1.5,0.2,2.2,0.4
	c1.6,0.6,3.5,0.7,4.3,0.7c0.2,0,0.8,0,1-0.1V10.2z"/>
<g id="Food">
	<g id="Layer_12">
		<g>
			<path fill="none" stroke="#868686" d="M48.1,3.5h3.7c2.5,0,4.5,2,4.5,4.5c0,0.5-0.4,0.9-0.9,0.9H44.5c-0.5,0-0.9-0.4-0.9-0.9
				C43.6,5.5,45.6,3.5,48.1,3.5z"/>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M43.5,8.7c-0.2,0.1-0.5,1.2,0,1.5c1.4,0.9,8.5,0.8,11.3,0.6
				c0.8-0.1,1.6-0.4,1.7-1.2c0-0.3-0.1-0.6-0.6-0.9"/>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M43.5,10.6L43.3,11c-0.2,0.5,0.2,1,0.7,0.9
				c0.3-0.1,0.5,0.1,0.7,0.3l0.1,0.2c0.3,0.5,1,0.6,1.5,0.2l0,0c0.3-0.2,0.7-0.3,1-0.2l0.8,0.3c0.4,0.1,0.8,0.1,1.2,0l0.5-0.2
				c0.4-0.2,0.9-0.2,1.3,0l0.5,0.2c0.4,0.2,0.8,0.1,1.2-0.1l0.2-0.1c0.3-0.2,0.8-0.1,1.1,0.1l0.2,0.2c0.3,0.3,0.8,0.2,1-0.2l0.1-0.2
				c0.1-0.2,0-0.3,0.2-0.4c0.5,0,1.2-0.3,1.1-0.7l-0.4-1.1"/>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M43.5,12.1c-0.1,0.2-0.3,0.8,0,1.1c0.3,0.4,3,1.1,6.4,1.1
				c2.2,0,4.6-0.3,6-0.6c0.5-0.1,0.9-0.4,0.8-0.9c0-0.2-0.2-0.5-0.4-0.7"/>
			<path fill="none" stroke="#868686" stroke-linecap="round" d="M43.5,13.3c0,0.5,0.6,2.4,1.3,2.6c1.8,0.8,5.7,0.7,8.1,0.5
				c1.3-0.1,2.5-0.7,3.2-1.8c0.3-0.5,0.5-1,0.5-1.4"/>
			<ellipse fill="#868686" cx="51.6" cy="6.5" rx="0.3" ry="0.4"/>
			<ellipse fill="#868686" cx="53" cy="4.9" rx="0.3" ry="0.4"/>
			<ellipse fill="#868686" cx="53" cy="7.2" rx="0.3" ry="0.4"/>
			<ellipse fill="#868686" cx="54.3" cy="6.5" rx="0.3" ry="0.4"/>
			<ellipse fill="#868686" cx="50.9" cy="4.9" rx="0.3" ry="0.4"/>
		</g>
	</g>
</g>
<g id="Animals">
	<g>
		<path fill="none" stroke="#868686" d="M24.2,11V3.5c0.1,0.1,0.8,0.9,2.8,3.1c2.5-1.7,5.6-0.7,6.9,0l2.4-3.1v7.1
			c0,1.2-0.1,2.5-0.9,3.4c-1,1.2-2.7,2.5-5.3,2.5c-2.9,0-4.5-1.5-5.3-2.9C24.2,12.9,24.2,11.9,24.2,11z"/>
		<path fill="none" stroke="#868686" stroke-linecap="round" d="M21.2,10l5.4,1.2"/>
		<path fill="none" stroke="#868686" stroke-linecap="round" d="M21.2,14.1l5.4-1.2"/>
		<path fill="none" stroke="#868686" stroke-linecap="round" d="M38.8,10l-5.4,1.2"/>
		<path fill="none" stroke="#868686" stroke-linecap="round" d="M38.8,14.1l-5.4-1.2"/>
		<path fill-rule="evenodd" clip-rule="evenodd" fill="#868686" d="M29.5,12.4L29,11.7c-0.2-0.3,0-0.6,0.3-0.6h1.4
			c0.3,0,0.5,0.4,0.3,0.6l-0.7,1l0,0c-0.7,1.2-2.6,1.1-3.1-0.3l-0.1-0.2c-0.1-0.2,0-0.4,0.2-0.5s0.4,0,0.5,0.2l0.1,0.2
			C28.3,12.7,29.1,12.9,29.5,12.4z"/>
		<path fill="none" stroke="#868686" stroke-linecap="round" d="M32.4,12.1l-0.1,0.2c-0.4,1-1.8,1.1-2.3,0.2"/>
		<ellipse fill="#868686" cx="27.6" cy="9.7" rx="0.7" ry="0.7"/>
		<ellipse fill="#868686" cx="32.4" cy="9.7" rx="0.7" ry="0.7"/>
	</g>
</g>
</svg>"##;
/// Arama büyüteci (20×40; üst satır).
pub const MAGNIFIER: &str = r##"<svg version="1.1" id="Layer_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px"
	 width="20px" height="40px" viewBox="0 0 20 40" enable-background="new 0 0 20 40" xml:space="preserve">
<path fill-rule="evenodd" clip-rule="evenodd" fill="#868686" d="M12,8.81c0,2.08-1.68,3.76-3.76,3.76c-2.08,0-3.76-1.68-3.76-3.76
	c0-2.08,1.68-3.76,3.76-3.76C10.32,5.05,12,6.73,12,8.81z M11.23,12.72c-0.83,0.64-1.87,1.01-2.99,1.01c-2.72,0-4.92-2.2-4.92-4.92
	c0-2.72,2.2-4.92,4.92-4.92c2.72,0,4.92,2.2,4.92,4.92c0,1.13-0.38,2.16-1.01,2.99l3.94,3.93c0.25,0.25,0.25,0.66,0,0.92
	c-0.25,0.25-0.66,0.25-0.92,0L11.23,12.72z"/>
<path fill-rule="evenodd" clip-rule="evenodd" fill="#C0C0BF" d="M12,28.81c0,2.08-1.68,3.76-3.76,3.76c-2.08,0-3.76-1.68-3.76-3.76
	c0-2.08,1.68-3.76,3.76-3.76C10.32,25.05,12,26.73,12,28.81z M11.23,32.72c-0.83,0.64-1.87,1.01-2.99,1.01
	c-2.72,0-4.92-2.2-4.92-4.92c0-2.72,2.2-4.92,4.92-4.92c2.72,0,4.92,2.2,4.92,4.92c0,1.13-0.38,2.16-1.01,2.99l3.94,3.93
	c0.25,0.25,0.25,0.66,0,0.92c-0.25,0.25-0.66,0.25-0.92,0L11.23,32.72z"/>
</svg>"##;
