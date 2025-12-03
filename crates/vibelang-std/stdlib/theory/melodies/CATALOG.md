# Melody Library - Quick Reference Catalog

Complete alphabetical listing of all 144 melodies with their import paths.

## Usage Pattern

```rhai
import "stdlib/theory/melodies/{category}.vibe" as alias;
let melody = alias::{function_name}();  // Returns string like "C4 E4 G4 ..."
```

---

## Complete Melody Index

### Classical (23)

| Melody | Function | Category |
|--------|----------|----------|
| Also Sprach Zarathustra | `movie::also_sprach_zarathustra()` | movie_themes |
| Blue Danube | `classical::blue_danube()` | classical |
| Canon in D | `classical::canon_in_d()` | classical |
| Carmen (Habanera) | `classical::carmen_habanera()` | classical |
| Clair de Lune | `classical::clair_de_lune()` | classical |
| Dance of the Sugar Plum Fairy | `classical::sugar_plum_fairy()` | classical |
| Eine Kleine Nachtmusik | `classical::eine_kleine_nachtmusik()` | classical |
| Für Elise | `classical::fur_elise()` | classical |
| Gymnopédie No. 1 | `classical::gymnopedie_1()` | classical |
| Hall of the Mountain King | `classical::hall_mountain_king()` | classical |
| Hallelujah Chorus | `classical::hallelujah_chorus()` | classical |
| Hungarian Dance No. 5 | `classical::hungarian_dance_5()` | classical |
| Minuet in G | `classical::minuet_in_g()` | classical |
| Moonlight Sonata | `classical::moonlight_sonata()` | classical |
| Nocturne Op. 9 No. 2 | `classical::nocturne_op9_no2()` | classical |
| Ode to Joy | `classical::ode_to_joy()` | classical |
| Ride of the Valkyries | `classical::ride_of_valkyries()` | classical |
| Spring (Vivaldi) | `classical::spring_vivaldi()` | classical |
| Swan Lake | `classical::swan_lake()` | classical |
| Symphony No. 40 (Mozart) | `classical::mozart_symphony_40()` | classical |
| Toccata and Fugue | `classical::toccata_fugue()` | classical |
| Turkish March | `classical::turkish_march()` | classical |
| William Tell Overture | `classical::william_tell_overture()` | classical |
| Winter (Vivaldi) | `classical::winter_vivaldi()` | classical |

### Folk Songs (15)

| Melody | Function | Category |
|--------|----------|----------|
| Amazing Grace | `folk::amazing_grace()` | folk_songs |
| Auld Lang Syne | `folk::auld_lang_syne()` | folk_songs |
| Camptown Races | `folk::camptown_races()` | folk_songs |
| Clementine | `folk::clementine()` | folk_songs |
| Danny Boy | `folk::danny_boy()` | folk_songs |
| Down by the Riverside | `folk::down_by_riverside()` | folk_songs |
| Greensleeves | `folk::greensleeves()` | folk_songs |
| Home on the Range | `folk::home_on_the_range()` | folk_songs |
| Oh! Susanna | `folk::oh_susanna()` | folk_songs |
| On Top of Old Smokey | `folk::old_smokey()` | folk_songs |
| Scarborough Fair | `folk::scarborough_fair()` | folk_songs |
| She'll Be Coming Round the Mountain | `folk::coming_round_the_mountain()` | folk_songs |
| Skip to My Lou | `folk::skip_to_my_lou()` | folk_songs |
| This Old Man | `folk::this_old_man()` | folk_songs |
| Yankee Doodle | `folk::yankee_doodle()` | folk_songs |

### Holiday Songs (10)

| Melody | Function | Category |
|--------|----------|----------|
| Auld Lang Syne | `holiday::auld_lang_syne_holiday()` | holiday_songs |
| Deck the Halls | `holiday::deck_the_halls()` | holiday_songs |
| Hark! The Herald Angels Sing | `holiday::hark_herald_angels()` | holiday_songs |
| Happy Birthday | `holiday::happy_birthday()` | holiday_songs |
| Jingle Bells | `holiday::jingle_bells()` | holiday_songs |
| Joy to the World | `holiday::joy_to_world()` | holiday_songs |
| O Christmas Tree | `holiday::o_christmas_tree()` | holiday_songs |
| Silent Night | `holiday::silent_night()` | holiday_songs |
| The First Noel | `holiday::first_noel()` | holiday_songs |
| We Wish You a Merry Christmas | `holiday::merry_christmas()` | holiday_songs |

### Jazz Standards (12)

| Melody | Function | Category |
|--------|----------|----------|
| Black Bottom Stomp | `jazz::black_bottom_stomp()` | jazz_standards |
| Blue Monk | `jazz::blue_monk()` | jazz_standards |
| Caravan | `jazz::caravan()` | jazz_standards |
| In the Mood | `jazz::in_the_mood()` | jazz_standards |
| Maple Leaf Rag | `jazz::maple_leaf_rag()` | jazz_standards |
| Sing Sing Sing | `jazz::sing_sing_sing()` | jazz_standards |
| St. Louis Blues | `jazz::st_louis_blues()` | jazz_standards |
| Summertime | `jazz::summertime()` | jazz_standards |
| Swing Low, Sweet Chariot | `jazz::swing_low_sweet_chariot()` | jazz_standards |
| Take Five | `jazz::take_five()` | jazz_standards |
| The Entertainer | `jazz::the_entertainer()` | jazz_standards |
| When the Saints Go Marching In | `jazz::when_saints_go_marching()` | jazz_standards |

### Movie Themes (15)

| Melody | Function | Category |
|--------|----------|----------|
| 2001: A Space Odyssey | `movie::also_sprach_zarathustra()` | movie_themes |
| Axel F (Beverly Hills Cop) | `movie::axel_f()` | movie_themes |
| Back to the Future | `movie::back_to_future_theme()` | movie_themes |
| Chariots of Fire | `movie::chariots_of_fire()` | movie_themes |
| Close Encounters (5 Notes) | `movie::close_encounters()` | movie_themes |
| Ghostbusters | `movie::ghostbusters_theme()` | movie_themes |
| Gonna Fly Now (Rocky) | `movie::gonna_fly_now()` | movie_themes |
| Halloween | `movie::halloween_theme()` | movie_themes |
| Imperial March (Star Wars) | `movie::imperial_march()` | movie_themes |
| Indiana Jones | `movie::indiana_jones_theme()` | movie_themes |
| James Bond | `movie::james_bond_theme()` | movie_themes |
| Jaws | `movie::jaws_theme()` | movie_themes |
| Superman | `movie::superman_theme()` | movie_themes |
| The Entertainer (The Sting) | `movie::the_entertainer_movie()` | movie_themes |
| The Good, the Bad and the Ugly | `movie::good_bad_ugly_theme()` | movie_themes |

### National Anthems (10)

| Country | Function | Category |
|---------|----------|----------|
| Australia | `anthems::advance_australia_fair()` | national_anthems |
| Belgium | `anthems::la_brabanconne()` | national_anthems |
| Canada | `anthems::o_canada()` | national_anthems |
| France | `anthems::la_marseillaise()` | national_anthems |
| Germany | `anthems::deutschlandlied()` | national_anthems |
| Italy | `anthems::fratelli_italia()` | national_anthems |
| Japan | `anthems::kimigayo()` | national_anthems |
| Mexico | `anthems::mexican_anthem()` | national_anthems |
| United Kingdom | `anthems::god_save_queen()` | national_anthems |
| United States | `anthems::star_spangled_banner()` | national_anthems |

### Nursery Rhymes (10)

| Melody | Function | Category |
|--------|----------|----------|
| Baa Baa Black Sheep | `nursery::baa_baa_black_sheep()` | nursery_rhymes |
| Humpty Dumpty | `nursery::humpty_dumpty()` | nursery_rhymes |
| Jack and Jill | `nursery::jack_and_jill()` | nursery_rhymes |
| London Bridge | `nursery::london_bridge()` | nursery_rhymes |
| Mary Had a Little Lamb | `nursery::mary_had_little_lamb()` | nursery_rhymes |
| Old MacDonald | `nursery::old_macdonald()` | nursery_rhymes |
| Pop Goes the Weasel | `nursery::pop_goes_weasel()` | nursery_rhymes |
| Row Row Row Your Boat | `nursery::row_row_row_boat()` | nursery_rhymes |
| Twinkle Twinkle Little Star | `nursery::twinkle_twinkle()` | nursery_rhymes |
| The Wheels on the Bus | `nursery::wheels_on_bus()` | nursery_rhymes |

### Pop/Rock (15)

| Melody | Function | Category |
|--------|----------|----------|
| Another One Bites the Dust | `rock::another_one_bites_dust()` | pop_rock |
| Billie Jean (bass) | `rock::billie_jean_bass()` | pop_rock |
| Come Together | `rock::come_together_riff()` | pop_rock |
| Day Tripper | `rock::day_tripper_riff()` | pop_rock |
| House of the Rising Sun | `rock::house_rising_sun()` | pop_rock |
| Iron Man | `rock::iron_man_riff()` | pop_rock |
| La Bamba | `rock::la_bamba()` | pop_rock |
| Louie Louie | `rock::louie_louie()` | pop_rock |
| Pretty Woman | `rock::pretty_woman_riff()` | pop_rock |
| Satisfaction | `rock::satisfaction_riff()` | pop_rock |
| Seven Nation Army | `rock::seven_nation_army()` | pop_rock |
| Smoke on the Water | `rock::smoke_on_water()` | pop_rock |
| Sunshine of Your Love | `rock::sunshine_love_riff()` | pop_rock |
| Superstition | `rock::superstition_riff()` | pop_rock |
| Sweet Child O' Mine | `rock::sweet_child_opening()` | pop_rock |

### TV Themes (15)

| Show | Function | Category |
|------|----------|----------|
| Cheers | `tv::cheers_theme()` | tv_themes |
| Doctor Who | `tv::doctor_who_theme()` | tv_themes |
| I Love Lucy | `tv::i_love_lucy_theme()` | tv_themes |
| Inspector Gadget | `tv::inspector_gadget_theme()` | tv_themes |
| Mission: Impossible | `tv::mission_impossible_theme()` | tv_themes |
| Pink Panther | `tv::pink_panther_theme()` | tv_themes |
| Scooby-Doo | `tv::scooby_doo_theme()` | tv_themes |
| Sesame Street | `tv::sesame_street_theme()` | tv_themes |
| Star Trek | `tv::star_trek_theme()` | tv_themes |
| The Addams Family | `tv::addams_family_theme()` | tv_themes |
| The Brady Bunch | `tv::brady_bunch_theme()` | tv_themes |
| The Flintstones | `tv::flintstones_theme()` | tv_themes |
| The Simpsons | `tv::simpsons_theme()` | tv_themes |
| The Twilight Zone | `tv::twilight_zone_theme()` | tv_themes |
| The X-Files | `tv::x_files_theme()` | tv_themes |

### Video Games (20)

| Game | Function | Category |
|------|----------|----------|
| Castlevania | `vg::castlevania_vampire_killer()` | video_games |
| Chrono Trigger | `vg::chrono_trigger_theme()` | video_games |
| Contra | `vg::contra_jungle()` | video_games |
| Donkey Kong | `vg::donkey_kong_theme()` | video_games |
| Duck Hunt | `vg::duck_hunt_theme()` | video_games |
| Final Fantasy | `vg::ff_victory_fanfare()` | video_games |
| Frogger | `vg::frogger_theme()` | video_games |
| Galaga | `vg::galaga_theme()` | video_games |
| Kirby (Green Greens) | `vg::kirby_green_greens()` | video_games |
| Legend of Zelda | `vg::zelda_theme()` | video_games |
| Mega Man | `vg::megaman_theme()` | video_games |
| Metroid | `vg::metroid_theme()` | video_games |
| Mike Tyson's Punch-Out | `vg::punch_out_theme()` | video_games |
| Pac-Man | `vg::pacman_theme()` | video_games |
| Pokemon | `vg::pokemon_theme()` | video_games |
| Sonic (Green Hill Zone) | `vg::sonic_green_hill()` | video_games |
| Space Invaders | `vg::space_invaders()` | video_games |
| Street Fighter 2 (Ryu) | `vg::street_fighter_ryu()` | video_games |
| Super Mario Bros | `vg::mario_theme()` | video_games |
| Tetris | `vg::tetris_theme()` | video_games |

### World Music (14)

| Origin | Function | Category |
|--------|----------|----------|
| Australia | `world::waltzing_matilda()` | world_music |
| Cuba | `world::guantanamera()` | world_music |
| France | `world::frere_jacques()` | world_music |
| Greece | `world::zorbas_dance()` | world_music |
| Ireland | `world::irish_washerwoman()` | world_music |
| Italy | `world::bella_ciao()` | world_music |
| Japan | `world::sakura_sakura()` | world_music |
| Jewish | `world::hava_nagila()` | world_music |
| Hebrew | `world::hatikvah()` | world_music |
| Mexico | `world::la_cucaracha()` | world_music |
| Russia | `world::kalinka()` | world_music |
| Scotland | `world::scotland_brave()` | world_music |
| South Africa | `world::siyahamba()` | world_music |
| Spain | `world::malaguena()` | world_music |

---

## Quick Search

**By Genre:**
- Classical → `classical.vibe` (23 melodies)
- Jazz/Ragtime → `jazz_standards.vibe` (12 melodies)
- Folk → `folk_songs.vibe` (15 melodies)
- Children → `nursery_rhymes.vibe` (10 melodies)
- Holiday → `holiday_songs.vibe` (10 melodies)
- World → `world_music.vibe` (14 melodies)

**By Era:**
- Pre-1900 → Classical, Folk Songs, World Music
- 1900-1950 → Jazz Standards, National Anthems
- 1950+ → Video Games, TV Themes, Movie Themes, Pop/Rock

**By Mood:**
- Uplifting → Ode to Joy, When the Saints, Happy Birthday
- Mysterious → Pink Panther, X-Files, Halloween
- Heroic → William Tell, Imperial March, Superman
- Peaceful → Silent Night, Clair de Lune, Gymnopédie
- Energetic → Tetris, Mario, Take Five

---

**Total: 144 melodies**
**Last Updated:** November 2025

